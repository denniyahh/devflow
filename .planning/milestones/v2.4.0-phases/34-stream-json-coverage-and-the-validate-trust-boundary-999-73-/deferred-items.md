# Phase 34 — deferred items

Out-of-scope discoveries recorded during execution, not fixed. Each names what
was measured and the control that makes the measurement mean something.

---

## 1. Widening `STREAM_JSON_STAGES` breaks 5 INTEGRATION tests (found by 34-06, blocks 34-05)

**Status:** open — needs a decision before plan 34-05 widens the constant.

Plan 34-06's acceptance is scoped to `cargo test -p devflow --bin devflow`, which
is green under a full five-stage widening after 34-06's repairs. **That suite does
not compile the integration tests.** `scripts/check.sh all` does, and under the
same widening `crates/devflow-cli/tests/phase7_cli.rs` fails:

| State | `phase7_cli` result | `check.sh all` exit |
|---|---|---|
| `STREAM_JSON_STAGES = &[Stage::Code]` (committed) | **17 passed; 0 failed** | **0** |
| widened to all five `Stage` variants | **12 passed; 5 failed** | **101** |

Same tree, same commit, only the constant differs — so this is caused by the
widening, not pre-existing. The unwidened row is the negative control; without it
"5 failed" would be consistent with a suite that was already red.

Failing tests, all with the same delivery-canary refusal at `Stage::Define`
(`background-task notification delivery is ABSENT`):

- `parallel_creates_two_worktrees_and_spawns_two_monitors`
- `start_defaults_to_worktree`
- `start_no_worktree_uses_feature_branch`
- `start_until_plan_halts_cleanly`
- `start_worktree_mode_ignores_main_checkout_divergence`

**Why 34-06 did not fix these.** Two reasons, both deliberate:

1. **Scope.** The plan's `files_modified` and task 2's `<files>` name only
   `pipeline_launch.rs` (plus the accessor sweep's five modules). These are a
   different target (`tests/phase7_cli.rs`), reached by a command the plan's
   acceptance criteria never run.
2. **The obvious repair may be the wrong one, and the plan says so.** These are
   end-to-end tests that spawn the real `devflow` binary, so the analogue of
   34-06's opt-out repair is setting `DEVFLOW_CLAUDE_LEGACY_LAUNCH=true` on the
   spawned command (`devflow_core::config::claude_legacy_launch`). For four of
   the five that is plausibly right — their subjects are worktree creation,
   feature-branch creation and `--until` halting, all orthogonal to the launch
   path. **`parallel_creates_two_worktrees_and_spawns_two_monitors` is not
   obviously in that class**: the delivery canary exists precisely to guarantee
   the multi-plan wave does not silently orphan concurrent work, so pinning that
   test to the legacy path may delete the coverage the canary was added to
   provide. 34-06's plan explicitly names this failure mode — "a test repaired by
   the wrong mechanism silently deletes coverage while turning the suite green" —
   and it is a judgment call that belongs to whoever owns the canary's contract,
   not to an executor working outside its plan's scope.

**Bearing on 34-05.** 34-06's stated purpose is that 34-05 "spends its budget on
evidence rather than on an unbudgeted repair discovered mid-run under a
live-capture time constraint." That purpose is **only partly discharged**: the
binary suite is safe, the integration suite is not. 34-05 will hit this unless it
is resolved first or explicitly budgeted for.

---

## 2. `embedded_commit_is_stale_resolves_execution_root_under_a_hostile_git_dir` is flaky (pre-existing)

**Status:** open — pre-existing, unrelated to 34-06's change, not fixed.

Observed failing 3 times across ~12 full-suite runs during 34-06, at both the
widened and unwidened state, and passing in isolation
(`1 passed`, 278 filtered out).

**Mechanism, demonstrated rather than inferred.** The test spawns a child copy of
the test binary (`std::process::current_exe`) to carry a hostile `GIT_DIR` in a
real process environment. The child inherits the parent process's `PATH` at spawn
time, and the outer half does **not** hold `ENV_MUTEX` while it spawns. A
concurrent test holding `ENV_MUTEX` and a `NeutralPath` guard replaces `PATH`
with a tempdir and then deletes that tempdir on drop, so the child can inherit a
`PATH` on which `git` is unresolvable; `embedded_commit_is_stale` then returns
`Indeterminate` instead of `Stale`.

Reproduced directly, outside the suite, with a two-commit fixture repo and the
test binary run in its inner mode:

| Child `PATH` | Result |
|---|---|
| inherited/normal | `1 passed` |
| a directory that does not exist | **`FAILED`, `left: Indeterminate, right: Stale`, `staleness.rs:1039`** |

The second row reproduces the observed flake's panic message and line number
exactly; the first is the control that shows the harness is not simply broken.

**Not attributable to 34-06's `env_lock` accessor.** The test never acquires
`ENV_MUTEX`, and the two tests that deliberately panic
(`reap_guard_reaps_the_monitor_when_a_later_assertion_panics`,
`trailing_reap_call_is_skipped_when_a_later_assertion_panics`) do so inside
`catch_unwind` without holding the lock, so they never poison it. Rate counts
were also taken — 0 failures in 5 baseline runs vs 2 in 7 with the change — but
those do **not** distinguish the two trees (Fisher exact p ≈ 0.47) and should not
be read as evidence either way. The mechanism above is the actual finding; the
counts are too weak to carry a conclusion.

**Plausible fix, not applied:** have the outer half acquire `ENV_MUTEX` for the
duration of the child spawn, so no `NeutralPath` region can be live concurrently.
This is the same family as the trailing-`PATH`-restore hazard `NeutralPath`'s doc
comment already describes, and as ROADMAP line 1472's recorded cascade.

---

## Milestone-close acknowledgment (v2.8.0, 2026-09-02)

All entries above acknowledged as deferred debt at the v2.8.0 milestone close, resolving
`audit-open`'s carried-forward flag on this file. `audit-open acknowledge --category
deferred_items` could not machine-match these entries (multi-paragraph text exceeding its
matcher's expected shape) — recorded manually per the documented fallback for that failure
mode, not because the content itself changed. None of these items block v2.8.0; they were
already historical record from the v2.4.0 close.
