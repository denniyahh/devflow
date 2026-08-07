---
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
plan: 02
subsystem: pipeline-launch
tags: [testing, worktree, checkpoint, regression-test, 999.84, HARDEN-04]
status: complete

requires:
  - "crates/devflow-core/src/verify.rs::phase_has_blocking_human_checkpoint"
  - "crates/devflow-cli/src/test_support.rs::{env_lock, init_repo, stub_agent_binary, prepend_path, ReapMonitorOnDrop}"
provides:
  - "crates/devflow-cli/src/pipeline_launch.rs::advance_with_worktree_declared_checkpoint_reads_the_execution_root"
  - "crates/devflow-cli/src/pipeline_launch.rs::write_plan_without_checkpoint"
affects:
  - "crates/devflow-cli/src/pipeline_launch.rs"

tech-stack:
  added: []
  patterns:
    - "opposite-result assertion inside the same test (verify.rs:351/:377 idiom, 34/D-08)"
    - "decoy fixture at the non-read root, so a wrong-root read fails for the right reason (D-05)"
    - "pre-written abort gate response so a fall-through path returns instead of blocking"

key-files:
  created: []
  modified:
    - "crates/devflow-cli/src/pipeline_launch.rs"

decisions:
  - "Revert form applied: rebinding `execution_root` to `project_root` at pipeline_launch.rs:1068 (not the call-site-argument form at :1070)"
  - "Added a sibling test rather than extending the base in place, so no-worktree call-site coverage is not traded away"
  - "Added `write_abort_gate_response` to the new fixture — without it the reverted run HANGS instead of failing"

metrics:
  duration: "~35 min"
  completed: 2026-08-07

actuals:
  tokens: 2290
  tasks: 2
  commits: 2
---

# Phase 35 Plan 02: Worktree-Mode GateReview Checkpoint Root Regression Test — Summary

A regression test now pins the `Action::GateReview` checkpoint auto-decide call site's use of
`execution_root`, and the argument was actually reverted and watched to fail — the test is not
merely asserted to discriminate, it was observed doing so.

## What Was Built

**`advance_with_worktree_declared_checkpoint_reads_the_execution_root`**
(`crates/devflow-cli/src/pipeline_launch.rs`) — a sibling of
`advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records`, which is left
byte-unchanged. It drives a real synchronous `advance()` through the `Action::GateReview` arm with:

- `state.worktree_path` set to a plain `create_dir_all` directory (D-05 — the argument under test
  resolves a path, and a linked worktree's files are ordinary files);
- the `blocking-human` PLAN present **only** under that worktree;
- a **decoy** PLAN under `project_root` for the same phase, identical in discovery shape, declaring
  the plain `blocking` gate;
- D-06's mechanical opposite-result control asserted in the same test, before `advance()`.

**`write_plan_without_checkpoint`** — the decoy fixture helper, written beside
`write_declared_checkpoint_plan` and using a new `PLAIN_GATE_VALUE_FOR_TEST` const so the
human-blocking literal never appears in it.

## Call-Site Citations, Re-Derived

The plan warned that the briefing's `:1070`/`:1071` pair was off by two. Re-verified in the
committed tree at the end of this plan (unchanged by this work, which is entirely below line 2240):

- `:1068` — `let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);`
- `:1070` — `&& verify::phase_has_blocking_human_checkpoint(execution_root, phase)`

## Task 2 — The Performed Revert

**Revert form applied: rebinding at `:1068`.** Chosen over passing `project_root` directly at the
call on `:1070` because the latter would leave `execution_root` bound-but-unused, adding an
`unused_variables` warning that is noise rather than signal.

```diff
-            let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
+            let execution_root = project_root;
```

### Failing output, verbatim (reverted tree)

```
   Compiling devflow v2.4.0 (/var/home/denniyahh/Github/devflow/.claude/worktrees/agent-ac1663c0a172d05e6/crates/devflow-cli)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.66s
     Running unittests src/main.rs (target/debug/deps/devflow-0516e76f01d97c62)

running 1 test
test pipeline_launch::tests::advance_with_worktree_declared_checkpoint_reads_the_execution_root ... FAILED

failures:

---- pipeline_launch::tests::advance_with_worktree_declared_checkpoint_reads_the_execution_root stdout ----
stage code finished with status Failed
  detail: checkpoint pending
gate written: .devflow/gates/94-code.json — awaiting response
workflow aborted for phase 94: abort: test cleanup

thread 'pipeline_launch::tests::advance_with_worktree_declared_checkpoint_reads_the_execution_root' (2140984) panicked at crates/devflow-cli/src/pipeline_launch.rs:2496:9:
assertion `left == right` failed: expected exactly one checkpoint_auto_decided event — the declaration lives in the worktree, so the arm must read the EXECUTION root: []
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    pipeline_launch::tests::advance_with_worktree_declared_checkpoint_reads_the_execution_root

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 279 filtered out; finished in 0.03s

error: test failed, to rerun pass `-p devflow --bin devflow`
```

The failure is the right failure: zero `checkpoint_auto_decided` events, and the run instead wrote
`.devflow/gates/94-code.json` and fell through to the generic gate — the exact 999.76 symptom.

### Localisation control (same reverted tree)

The no-worktree sibling still passed under the identical revert:

```
running 1 test
test pipeline_launch::tests::advance_with_declared_checkpoint_and_reported_gate_relaunches_and_records ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 279 filtered out; finished in 0.08s
```

This is what localises the failure to **root selection** rather than to checkpoint machinery in
general. Had both failed, the revert would only have shown that the arm was broken somehow.

### Passing output, verbatim (restored tree)

```
   Compiling devflow v2.4.0 (/var/home/denniyahh/Github/devflow/.claude/worktrees/agent-ac1663c0a172d05e6/crates/devflow-cli)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.27s
     Running unittests src/main.rs (target/debug/deps/devflow-0516e76f01d97c62)

running 1 test
test pipeline_launch::tests::advance_with_worktree_declared_checkpoint_reads_the_execution_root ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 279 filtered out; finished in 0.08s
```

Restoration confirmed mechanically: `git diff --exit-code -- crates/devflow-cli/src/pipeline_launch.rs`
exited 0, and `git status --porcelain` on that path printed nothing.

### D-06's distinction, stated

The two halves establish different things and neither replaces the other:

- **The mechanical assertion inside the test** proves the two roots *disagree for this fixture* —
  `phase_has_blocking_human_checkpoint` is `true` for the worktree and `false` for the project
  root. That disagreement is what makes the revert meaningful; without it, a revert could "fail"
  merely because no PLAN existed anywhere. It re-runs on every `cargo test`.
- **The performed revert** is the only thing that establishes that the call site actually passes
  `execution_root`. The mechanical assertion cannot show that — it calls the predicate directly and
  never observes which argument the production arm supplies. But it is a one-time act that nothing
  re-runs.

## What This Does NOT Establish

Carried from `35-VALIDATION.md` and the plan's `must_haves`, stated so nobody reads more into it:

1. **Nothing here establishes git-worktree-specific semantics.** The fixture is a plain
   `create_dir_all` directory standing in for a worktree (D-05). It exercises path resolution, which
   is what the argument under test does. Shared refs, linked-worktree `.git` files, index
   separation, and every other property of a real `git worktree add` are untested by this work.
   999.76's open question about a real linked-worktree harness remains open.
2. **The arm's other four preconditions are held constant and unexercised.** The fixture pins agent
   kind (`Claude`), capture confirmation, session id presence, and the resume ceiling to their
   satisfying values throughout. This test discriminates on the **root-selection axis only**; it
   says nothing about the behaviour of the other four conditions, which are covered by the
   pre-existing siblings at `advance_without_declared_checkpoint_falls_through_to_generic_gate`,
   `..._but_unreported_gate_falls_through`, `..._and_no_session_id_falls_through`, and
   `advance_with_non_claude_agent_never_resumes`.
3. **A single observed failure is a single observation.** The revert was performed once, in one
   form, on one host. It establishes that this test discriminates against that specific reverted
   expression — not that it would catch every conceivable future refactor of the same call site.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking] The plan's verify command targets a `--lib` that does not exist**

- **Found during:** Task 1 verification
- **Issue:** The plan's `<automated>` blocks specify `cargo test -p devflow --lib …`. The `devflow`
  package has no library target — `crates/devflow-cli/Cargo.toml` declares `name = "devflow"` with
  only a binary. The command fails outright with
  `error: no library targets found in package 'devflow'`, so it could never have verified anything.
- **Fix:** Used `cargo test -p devflow --bin devflow …` throughout. The package name is still
  `devflow`, exactly as `CLAUDE.md` warns; only the target kind was wrong.
- **Files modified:** none (command-line only)
- **Note for future plans in this phase:** plans 35-01 and 35-03…35-06 carry the same `--lib`
  formulation in their verify blocks if they target this crate. It will fail the same way.

**2. [Rule 3 — Blocking] The reverted run HUNG instead of failing; fixture needed an abort gate response**

- **Found during:** Task 2, first revert attempt
- **Issue:** With the call site reverted, the arm falls through to the never-silent gate, and
  `run_gate` polls indefinitely for an operator response that no test writes. The first reverted run
  was still executing after 600s and had to be killed with `SIGTERM`; cargo reported
  `signal: 15, SIGTERM` rather than a test failure. **An unbounded hang is strictly weaker evidence
  than a failure** — it cannot distinguish a failed assertion from a wedged harness, and the
  acceptance criteria require a real `test result: FAILED` with `1 failed`.
- **Root cause:** The new test omitted `write_abort_gate_response(root, phase, Stage::Code)`. All
  three pre-existing negative siblings write it for exactly this reason; the positive base test does
  not need it because its auto-decide path returns before `run_gate`, and I modelled the new test on
  the base without noticing that the *reverted* form takes the fall-through path.
- **Fix:** Added `write_abort_gate_response(root, phase, Stage::Code)` to the fixture, with a comment
  recording why. It is inert on the path the test asserts (nothing reads it when auto-decide returns
  early), and the test passes unchanged with it present. The revert was then re-performed and
  produced the verbatim failure recorded above.
- **Files modified:** `crates/devflow-cli/src/pipeline_launch.rs`
- **Commit:** `adf0e5f`
- **Committed separately rather than amended**, so the history records that the first fixture could
  not have produced the required evidence.

### Not Fixed (out of scope)

None. No pre-existing failures or warnings were encountered.

## Verification

| Check | Result |
|---|---|
| New test, named, exact | `test result: ok. 1 passed; 0 failed; … 279 filtered out` — non-zero `filtered out` confirms a real name match |
| Base test byte-unchanged | `git diff -U0` shows three hunks, all `-NNNN,0` (pure insertions); base test spans the untouched region |
| Base test still green | `1 passed; … 279 filtered out` |
| `pipeline_launch::` module | `test result: ok. 32 passed; 0 failed; … 248 filtered out` |
| Test count increased | `#[test]` in `pipeline_launch.rs`: 31 at base `749a151` → 32 now; that file is the only file changed since base (`git diff --stat`) |
| Whole binary | `test result: ok. 280 passed; 0 failed` |
| `scripts/check.sh all` | `==> check.sh: all OK` — fmt clean, clippy clean under `-D warnings`, `0 failed` across the workspace |
| Revert fully restored | `git diff --exit-code` → 0; `git status --porcelain <path>` → empty |
| No `35-evidence/` directory | `ls` → `No such file or directory` |
| Decoy helper present | `rg -n 'fn write_plan_without_checkpoint'` → line 2280 |
| Decoy body lacks the human gate | `rg 'HUMAN_GATE_VALUE_FOR_TEST'` in the decoy body → 0 hits; **negative control**: same grep in `write_declared_checkpoint_plan` → 1 hit |
| New test has no thread/polling | `rg 'thread::scope\|std::thread\|for _ in 0\.\.\|sleep\|loop {'` in the new test body → 0 hits; **negative control**: same grep against the wrong base `code_unknown_does_not_transition_to_validate` → 3 hits, so the grep can fire |

Two of these carry explicit negative controls because a clean first-try result with no opposite case
is exactly the shape of a proxy measurement.

## Known Stubs

None.

## Threat Flags

None. This plan adds test code only; no production behaviour changed, and `T-35-09` (the
temporarily-reverted working tree) was mitigated as planned — the revert was never committed, and
`git diff --exit-code` on the file is recorded above.
