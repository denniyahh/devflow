---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
reviewed: 2026-08-04T23:30:00Z
depth: standard
files_reviewed: 7
files_reviewed_list:
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/prompt.rs
  - crates/devflow-core/src/state.rs
findings:
  critical: 1
  warning: 6
  info: 5
  total: 12
status: issues_found
---

# Phase 33: Code Review Report

**Reviewed:** 2026-08-04
**Depth:** standard
**Status:** issues_found
**Scope:** `7b55fcefb8d047bd00db7db6a1365664ffb25acc..HEAD`, the 7 source files listed above
(33-01 through 33-04). Re-review after 33-04's test-only gap closure.

## Summary

**The headline finding is new and is a BLOCKER: D-01's entire decision signal is read from the
wrong working tree.** `select_loop_back_fix` asks `phase_verification_exists(project_root, …)`,
but in worktree mode — DevFlow's normal operating shape — the Validate agent authors
`{N}-VERIFICATION.md` inside the phase's *worktree*, while `project_root` is the main checkout
sitting on `develop`, where that file does not exist. The predicate therefore returns `false`
for every in-flight phase in worktree mode, `FixType::GapsOnly` becomes unreachable on the
Validate path, and the D-02 negative control that phase 33 shipped
(`genuine_gaps_loop_back_still_issues_gaps_only`) passes only because it seeds the artifact under
a root with no worktree configured. This codebase already has the correct idiom five files away:
`archive_phase_files` takes a separate `evidence_root` argument (`state.worktree_path.as_deref()
.unwrap_or(&state.project_root)`) for exactly this reason. See CR-01.

The 999.66 counter logic itself is boundary-correct. I traced the reset/accumulate arms at
`consecutive_failures ∈ {0, 1, 2, MAX-1, MAX, u32::MAX}` and the `None` / equal / lower baseline
cases: three no-progress failures still reach the gate (no off-by-one against pre-33 behavior),
`saturating_add` bounds the overflow path, and the baseline is written on **every** recorded
Failed outcome regardless of which arm ran. `transition()` deliberately does not clear the
baseline; I checked the resulting stale-baseline paths (Validate→Ship→LoopBack, abort/restart,
per-phase state file isolation) and none of them produce an unsafe outcome — they fail toward
gating. One exception: WR-03, where a *transient* git failure hands out a free counter reset on
the following cycle, contradicting the doc comment's stated safety direction.

**33-04's own claim checks out.** I independently enumerated `consecutive_failures` assignment
sites workspace-wide: 14 total (2 production, 12 test), matching the SUMMARY. Every test site that
drives `handle_validate_outcome` now seeds `last_validate_failure_commit_count`; the sites that do
not seed it (`resource_killed_*`, `repeated_code_to_validate_transition_*`,
`consecutive_failures_are_independent_across_phases`, `state.rs`'s serde test) never reach the
reset-vs-accumulate branch. No integration test under `crates/*/tests/` seeds the counter at all.
So no *currently reachable* real-agent launch remains — but the hardening 33-04 applied is uneven,
which is WR-06.

**What I verified, and what that does not establish.** I ran the 8 phase-33 regression tests
(`cargo test -p devflow --bin devflow`) — all 8 pass. That establishes the non-worktree paths
work; it establishes nothing about CR-01, because no test in the workspace configures
`state.worktree_path` on a `handle_validate_outcome` drive. I reproduced CR-01's premise directly
with a scratch repo + linked worktree, including a negative control in the same probe: the
`.planning/` artifact is invisible from the main checkout (`NO`) while `git rev-list --count
develop..feature/phase-33` from that same main checkout correctly reads `1` — i.e. the two
adjacent reads in `handle_validate_outcome` genuinely require different roots, and only the commit
count is using the right one. I did not build a running DevFlow in worktree mode end-to-end.

Prior-review status: **WR-01 still open** (unchanged, now with stronger evidence), **WR-02 still
open** (partly superseded by CR-01), **IN-01 still open**, **IN-02 still open**. 33-04 was
test-only and touched none of them.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01 (BLOCKER): `select_loop_back_fix` reads `.planning/` from the main checkout, so D-01's signal is always `false` in worktree mode

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:243-249` (call sites `:291`, `:343`, `:354`);
`crates/devflow-core/src/agent_result.rs:2578-2596`

**Issue:** `phase_verification_exists(project_root, phase)` stats
`project_root/.planning/phases/{NN}-*/{NN}-VERIFICATION.md`. The `project_root` threaded into
`handle_validate_outcome` is the main checkout (`advance(project_root, …)`,
`pipeline_launch.rs:836`, and `state.project_root`). But the agent that writes that artifact runs
in the worktree — `monitor.rs:313-320` states it outright: *"The agent runs in its worktree when
worktree mode is active … Capture/state files and the `devflow check` calls below always use the
main project root."* `.planning/` is tracked, so the file is committed on `feature/phase-NN` and
is simply absent from the main checkout's tree while that phase is in flight.

Consequence: in worktree mode every Validate→Code loop-back — the plain tail arm, the
consecutive-failure gate arm, and the Ambiguous gate arm — dispatches `FixType::FullExecute`,
even for a phase that has been verified and genuinely has gaps. And the mis-dispatch is not
harmless: `/gsd-execute-phase N` against a phase whose plans are all complete does nothing and
commits nothing, which `evaluate_layer2`'s `no_work_done` gate classifies as a Code-stage
`Failed`, routing to `handle_stage_failure`'s never-silent gate. A gaps loop that should have
self-healed instead parks on a human gate — the same *class* of unresolvable stall D-01 was
written to remove, arrived at from the other direction.

Reproduced (with negative control, scratch repo + linked worktree):

```
== worktree (agent cwd) sees it:            YES
== main checkout (project_root) sees it:    NO
== commit count from main checkout:         1     <- control: this read IS correct from here
```

The shipped test `genuine_gaps_loop_back_still_issues_gaps_only`
(`pipeline_outcomes.rs:1483-1526`) writes the artifact under `root` and never sets
`state.worktree_path`, so it cannot fail on this. Green tests over an inverted decision.

**Fix:** thread the evidence root, exactly as `archive_phase_files` already does
(`pipeline_launch.rs:569-577`):

```rust
// pipeline_outcomes.rs
fn select_loop_back_fix(evidence_root: &Path, phase: u32) -> FixType {
    if agent_result::phase_verification_exists(evidence_root, phase) {
        FixType::GapsOnly
    } else {
        FixType::FullExecute
    }
}

// at all three call sites, in handle_validate_outcome:
let evidence_root = state
    .worktree_path
    .as_deref()
    .unwrap_or(project_root)
    .to_path_buf();
… select_loop_back_fix(&evidence_root, state.phase) …
```

Note the two reads must stay on *different* roots: `phase_commit_count` must keep taking
`project_root` (refs and the object DB are shared, so the main checkout is authoritative and the
worktree's own `.git` file is unreadable from some contexts — see CLAUDE.md's pre-push-gate note).
Rename the `select_loop_back_fix` parameter to `evidence_root` so the distinction is visible at
the call site rather than implied.

Add a regression test that sets `state.worktree_path = Some(wt)`, writes `{N}-VERIFICATION.md`
**only** under `wt`, and asserts `last["fix"] == "GapsOnly"` — with the existing
non-worktree test kept as its negative control.

## Warnings

### WR-01 (WARNING, carried from the prior review — STILL OPEN): the forward-progress reset removes the pipeline's only unconditional bound on the Code↔Validate loop

**File:** `crates/devflow-core/src/mode.rs:149-151`; `crates/devflow-cli/src/pipeline_outcomes.rs:300-325`

**Issue:** unchanged from the prior review — 33-04 was test-only and added no secondary ceiling,
no lines-changed threshold, and (confirmed by grep of `ROADMAP.md`) no numbered backlog entry
tracking the deferral. `consecutive_failures_made_progress` resets the streak to 1 whenever the
commit *count* rises, and the Code↔Validate loop has no other bound (`infra_failures`,
`preflight_retries`, `checkpoint_resumes` are the only other bounded counters and none fires on
this loop shape).

New evidence that strengthens it: on this repo the Code stage's fix command is a GSD command, and
GSD commands routinely commit `.planning/` artifacts (SUMMARY.md, STATE.md) even when they change
no source. So "commits something trivial on every cycle" is not an adversarial hypothetical here —
it is the ordinary behavior of the thing that runs in that slot. MEMORY.md's own
`project-devflow-commit-gate-not-scoped-to-stage` entry records the same signal already misfiring
in `agent_result.rs` for a related reason.

**Fix:** as before — either (a) a progress-independent per-phase Code↔Validate cycle ceiling, or
(b) strengthen the signal (require the commit range to touch a non-`.planning/` path, which would
cheaply exclude the routine case above). Whichever is chosen, file it as a numbered ROADMAP
backlog item and reference the number from `mode.rs`'s doc comment, so the deferral is tracked
rather than living only in prose.

### WR-02 (WARNING, carried — STILL OPEN, partly superseded by CR-01): `phase_verification_exists` has no staleness invalidation, and its doc comment still does not say so

**File:** `crates/devflow-core/src/agent_result.rs:2564-2596`

**Issue:** unchanged. Nothing in the workspace deletes or dates `{N}-VERIFICATION.md`; a phase
whose plan set grows after a first verification will keep choosing `GapsOnly` for plans that were
never judged. The prior review asked only for a doc-comment note naming the limitation; the doc
comment in HEAD is byte-identical to 33-01's, so that was not done. CR-01 is the more urgent
instance of "this predicate answers a question it cannot actually see," and a fix for CR-01 is a
natural place to also record this one.

**Fix:** add the limitation to the doc comment now; if a real fix follows, compare the artifact's
mtime (or a recorded plan count) against the phase's current plan set.

### WR-03 (WARNING, new): a transient `git` failure grants a free counter reset on the next cycle — the doc comment's stated safety direction only holds while git stays broken

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:255-268` (doc), `:300-323`;
`crates/devflow-core/src/agent_result.rs:1841-1861`

**Issue:** `phase_commit_count` returns `0` indistinguishably for "no commits", "branch missing",
and "git could not be run" — documented and fine on its own. But `handle_validate_outcome`'s doc
comment claims the resulting failure direction is toward gating: *"an unrunnable `git` or a
missing branch counts zero every cycle, so once a baseline is recorded the counter accumulates and
the gate stays reachable."* That is only true if git stays broken. A **single transient** failure
records `Some(0)` as the baseline; on the very next cycle git works again, reads the branch's real
count (say 40), and `40 > 0` reports **progress**, resetting `consecutive_failures` to 1 with no
new work having been done. One flaky `git` invocation is worth one free ceiling reset — the
opposite of what the comment promises, at the exact moment the promise matters.

**Fix:** distinguish "counted zero" from "could not count". Have `phase_commit_count` return
`Option<u32>` (or add a sibling that does), and treat `None` at the call site as *not progress*
without overwriting the baseline:

```rust
match agent_result::phase_commit_count_checked(project_root, &GitFlowConfig::default(), state.phase) {
    Some(current) => {
        if mode::consecutive_failures_made_progress(state.last_validate_failure_commit_count, current) {
            state.consecutive_failures = 1;
        } else {
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        }
        state.last_validate_failure_commit_count = Some(current);
    }
    // Could not measure: accumulate, and leave the baseline untouched so the
    // next successful measurement compares against the last REAL observation.
    None => state.consecutive_failures = state.consecutive_failures.saturating_add(1),
}
```

If that is judged out of scope, the doc comment at `:266-268` must be corrected — it currently
asserts a safety property the code does not have.

### WR-04 (WARNING, new): the gate context now under-reports the failure count in Supervise mode

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:313`, `:331-337`

**Issue:** the gate text is `"Validation failed {} time(s) — human review needed."` interpolating
`state.consecutive_failures`. Before 33-03 that counter was a genuine within-loop failure count
(neither `transition(Code, Validate)` nor `prepare_loop_back_to_code` resets it), so the message
was accurate. After 33-03 it is a *streak length*. In Supervise mode — where Validate gates on
**every** failure, not just at the ceiling — a phase whose Code stage commits something on each
cycle will show `"Validation failed 1 time(s)"` at the 2nd, 5th and 9th gate alike. The human
being asked to adjudicate is shown a number that materially understates how long this has been
going on, and the comment at `:308-312` explaining why the reset writes `1` rather than `0` shows
the message was considered but the Supervise case was not.

**Fix:** either interpolate a separate, never-reset per-phase Validate-failure total (which would
also give WR-01 its ceiling for free), or reword the context so the number is honest, e.g.
`"Validation failed {n} time(s) in the current streak"` plus the commit-count delta that produced
the reset.

### WR-05 (WARNING, new): PATH neutralization is restored by a trailing statement, not RAII — a panic inside the region corrupts PATH process-wide and poisons `ENV_MUTEX` for every later test

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:1313-1341` (and 9 further regions in this
file), `crates/devflow-cli/src/pipeline_gate.rs:1140-1159`, `:1290-1307`;
`crates/devflow-cli/src/test_support.rs:50` (`ENV_MUTEX`), `:112-137`
(`commit_on_feature_branch`)

**Issue:** every PATH-neutralized region has the shape `lock → set_var → work → set_var(restore)`,
where the restore is a plain trailing statement. Rust abandons the remaining statements of a
function the instant a panic begins unwinding, so any panic inside the region:

1. leaves `PATH` pointed at `neutral_path_dir`, which the unwind then **drops and deletes** —
   every other test thread in the same process (Rust's default runner is parallel) now has a PATH
   naming a nonexistent directory; and
2. poisons `ENV_MUTEX`, so every subsequent `ENV_MUTEX.lock().unwrap()` — 60 sites across the
   workspace — panics with `PoisonError`, converting one legible failure into a cascade.

This file's own `test_support.rs` documents this exact mechanism at length for a different
resource (`:348-355`, `:396-398`: *"a plain trailing call to `reap_spawned_monitor` only runs on
the success path … it is the language's own `Drop` guarantee — not a call ordering convention —
that makes the reap unconditional"*), and solved it there with `ReapMonitorOnDrop`. The PATH
regions never got the same treatment.

The pattern predates phase 33, but 33-01 introduced the **first** region that runs a multi-step,
multi-assert git fixture inside it: `healthy_multi_wave_progress_does_not_reach_the_ceiling`
(`:1320-1333`) calls `commit_on_feature_branch` — 3 `assert!`s and 2 `.unwrap()`s driving real
`git` — under the replaced PATH, on every one of 4 loop iterations. That materially raises the
probability of the failure mode, on a fixture whose git calls are themselves PATH-dependent.

**Fix:** an RAII guard in `test_support.rs`, modeled on `ReapMonitorOnDrop`, so the restore is
unconditional and the tempdir outlives the restore:

```rust
pub(crate) struct NeutralPath {
    _dir: tempfile::TempDir,
    original: Option<std::ffi::OsString>,
}

impl NeutralPath {
    /// Caller must already hold ENV_MUTEX.
    pub(crate) fn install() -> Self {
        let dir = agent_free_git_only_path_dir();
        let original = std::env::var_os("PATH");
        unsafe { std::env::set_var("PATH", dir.path()) };
        Self { _dir: dir, original }
    }
}

impl Drop for NeutralPath {
    fn drop(&mut self) {
        unsafe {
            match &self.original {
                Some(path) => std::env::set_var("PATH", path),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}
```

Separately, replace `ENV_MUTEX.lock().unwrap()` with a helper that recovers from poisoning
(`.unwrap_or_else(PoisonError::into_inner)`) — the mutex guards a `()`, so there is no invariant a
poisoned lock could be protecting, and recovering keeps one failing test from taking the rest
with it.

### WR-06 (WARNING, new): 33-04's hardening is applied to one of the four tests that need it

**File:** `crates/devflow-cli/src/pipeline_gate.rs:1111-1170` (hardened);
`crates/devflow-cli/src/pipeline_outcomes.rs:778-819`, `:831-888`, `:1835-1860` (not hardened)

**Issue:** 33-04 correctly identified that a test seeding `consecutive_failures` directly now
carries a `None` baseline, takes the reset arm, drops out of the gated path and falls through to
`loop_back_to_code` → `launch_stage` → a real `claude` spawn during `cargo test`. It fixed
`abort_cleans_up_gate_files_…` with three layers: the baseline seed, a neutralized PATH, and two
branch-pinning assertions. But the same three-layer treatment was not extended to the other three
sites that now depend on the identical invariant:

- `validate_failure_threshold_forces_gate_then_aborts` (`:785`, `:792`)
- `drive_validate_advance_and_read_gate_context` (`:839`, `:843`) — backing 3 tests
- `consecutive_failures_increment_saturates` (`:1842`, `:1846`)

All three got only the baseline seed. All three have a post-hoc assertion that would *notice* the
drift (`assert_eq!(state.consecutive_failures, MAX)`, the gate-file poll, `assert_eq!(…, u32::MAX)`)
— but in each case the real agent spawn happens *before* that assertion runs. A post-hoc detector
is not a preventer, and preventing the spawn is the whole point of the repair. `abort_cleans_up_…`
is the one that got it right; the doc comment there even says so.

**Fix:** wrap the `handle_validate_outcome` / `advance` call in each of the three with the same
`ENV_MUTEX` + neutralized-PATH block (ideally the `NeutralPath` guard from WR-05), so a future
drift fails at `ensure_agent_binary` instead of at a spawned CLI. This is ~6 lines per site and
makes the invariant enforced by construction rather than by three separate authors remembering it.

## Info

### IN-01 (INFO, carried — STILL OPEN): `select_loop_back_fix` has no direct unit test

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:243-249`

**Issue:** confirmed unchanged — grep shows the only references are the three call sites and three
doc-comment mentions. The decision is still reachable only through four `ENV_MUTEX`-serialized,
PATH-neutralized, full-`handle_validate_outcome` drives. A direct table test would also be the
cheapest place to pin CR-01's fix.

**Fix:** a two-case table test over `(worktree_path, artifact_location) → FixType`.

### IN-02 (INFO, carried — STILL OPEN): a resumed pre-999.66 `state.json` silently reads as a fresh failure streak, with no distinguishing event

**File:** `crates/devflow-core/src/state.rs:73-100`; `crates/devflow-cli/src/pipeline_outcomes.rs:300-313`

**Issue:** unchanged. `None` is emitted for both "genuine first failure" and "state predates this
field"; nothing in `events.jsonl` tells them apart, so an operator upgrading a binary mid-phase has
no signal that the effective failure budget just widened once.

**Fix:** as before — a distinct reason string on the `loop_back` event for the absent-baseline case.

### IN-03 (INFO, new): the branch-name derivation is still duplicated at the site the count was extracted from

**File:** `crates/devflow-core/src/agent_result.rs:1841-1842` and `:1904`

**Issue:** `phase_commit_count`'s doc comment says the extraction exists because two independent
copies of the count *"were able to silently diverge."* But `evaluate_layer2` still derives
`let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase)` itself, for its five error
messages, while `phase_commit_count` derives the same string internally from the same inputs. They
cannot diverge today, but the duplication is the same shape the extraction was meant to close, and
there are two further copies (`ship_evidence.rs:160`, `test_support.rs:122`).

**Fix:** expose `pub fn phase_branch_name(git_flow: &GitFlowConfig, phase: u32) -> String` and call
it from all four sites.

### IN-04 (INFO, new): `commit_on_feature_branch` leaves the repo checked out on the feature branch

**File:** `crates/devflow-cli/src/test_support.rs:112-137`

**Issue:** the helper does `git checkout {branch}` (or `-b`) and never restores the prior checkout.
Benign for its two current callers (neither runs a hook batch afterwards, and `hooks_for_transition(Code, Validate)`
is empty), but a future test that pairs this fixture with a `Validate → Ship` transition would run
`DocsUpdate`, `Merge` and `VersionBump` against the feature branch instead of `develop` and fail in
a way that points nowhere near this helper. The doc comment describes the `-B`-vs-`checkout` care
taken but not this leak.

**Fix:** capture `git rev-parse --abbrev-ref HEAD` at entry and restore it at exit, or at minimum
document the post-condition ("leaves HEAD on the feature branch").

### IN-05 (INFO, new): `fix_prompt`'s preamble contradicts what `FullExecute` means

**File:** `crates/devflow-core/src/prompt.rs:299-306`

**Issue:** all three variants share the preamble `"Validation reported issues. Run the fix command
for this loop:"`. For `FullExecute` that is precisely wrong by D-01's own framing — the phase is
mid-arc, *not* defective; validation reported that plans have not been judged yet, not that they
failed. The agent receiving this prompt is told to treat a normal continuation as a defect.

**Fix:** branch the preamble alongside the command:

```rust
let (preamble, command) = match fix_type {
    FixType::FullExecute => (
        "This phase still has unexecuted plans. Continue the phase:",
        format!("/gsd-execute-phase {phase}"),
    ),
    …
};
```

---

_Reviewed: 2026-08-04_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
_Re-review of the 2026-08-04 REVIEW.md at this path, after 33-04. Prior WR-01, WR-02, IN-01 and
IN-02 are all still open and are restated above with their status; CR-01, WR-03 through WR-06 and
IN-03 through IN-05 are new._
