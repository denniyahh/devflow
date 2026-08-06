---
phase: 33-loop-back-correctness-for-multi-wave-validate-code-cycles-99
reviewed: 2026-08-05T00:00:00Z
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
  warning: 8
  info: 7
  total: 16
status: issues_found
---

# Phase 33: Code Review Report

**Reviewed:** 2026-08-05
**Depth:** standard
**Files Reviewed:** 7
**Status:** issues_found
**Scope:** `7b55fcefb8d047bd00db7db6a1365664ffb25acc..HEAD` (the true `git merge-base develop HEAD`),
the 7 source files listed above. Third pass — after 33-05's CR-01 closure
(`12f12e6`, `e9a5eb2`).

## Summary

**The prior review's CR-01 is genuinely closed, not papered over.** All three Validate loop-back
arms now resolve the evidence root from `state.worktree_path`
(`pipeline_outcomes.rs:322`, `:378`, `:393`); a grep for `FixType::` confirms there is no fourth
Validate arm that was missed, and `phase_verification_exists` has exactly one production caller.
The deliberately-asymmetric second read — `phase_commit_count` still on `project_root`
(`:336`) — is correct and is now documented in **both** places that matter: the new caller comment
at `:288-298` and the callee's own pre-existing contract at `agent_result.rs:1833-1836` ("Must be
called with the main `project_root`, never a worktree path"). Two independent statements of the
same rule, which is what makes the asymmetry auditable rather than accidental.

The new test pair is a real discriminator, not a rubber stamp. It rules out three distinct wrong
implementations, each by a different case: "probe `project_root` only" fails the positive test
(phase 93); "return `GapsOnly` whenever a worktree is configured" fails scenario A (phase 94);
"probe both roots and OR them" fails scenario B (phase 95) and nothing else. Scenario B was not
required by the verification and is the one case that earns the pair its name.

**The headline finding is new and is a BLOCKER, and it is the same defect class CR-01 was.**
`evaluate_layer0` reads the PLAN's `external_verify` declaration with
`crate::verify::external_verify_commands(project_root, …)` (`agent_result.rs:2042`) while holding
the correctly-resolved `execution_root` on the line immediately above it (`:2041`), and its doc
comment at `:2025-2031` asserts the premise this phase's own new doc comment refutes: *"`.planning/
phases/` lives there, not in a worktree checkout."* That claim is false — `.planning/` is tracked,
so the phase's plans live on `feature/phase-N` and are invisible from a main checkout sitting on
`develop`. Measured, with a negative control on the same repo: `git ls-tree -r --name-only develop
-- .planning/phases | grep -c '33-'` returns **0**, while the identical command against `HEAD`
lists every one of phase 33's plan files. See CR-01.

**Carried findings.** WR-01, WR-02, WR-03, WR-04, WR-05, WR-06 and IN-01 through IN-05 from the
prior report are **all still open** — 33-05 touched none of them, which matches the 33-05
executor's own report. WR-05 is measurably *worse*: the two new tests add two more non-RAII
PATH-restore regions.

**What I verified, and what it does not establish.**

- `cargo test -p devflow --bin devflow -- worktree_mode genuine_gaps_loop_back_still_issues_gaps_only mid_arc_loop_back_issues_plain_execute_command`
  → `4 passed; 0 failed; 267 filtered out`. The non-zero `filtered out` is the guard against this
  repo's documented false-green: the names matched real tests.
- `cargo clippy --workspace --all-targets -- -D warnings` → exit 0. `cargo fmt --all -- --check` →
  exit 0.
- CR-01's premise was measured with a negative control (the `develop` vs `HEAD` `ls-tree` pair
  above), not inferred.

None of that establishes end-to-end behavior. Every test in this phase drives a tempdir with
`PATH` neutralized, so no real agent ever runs. More narrowly: the two new "worktree mode" tests
create their worktree with `std::fs::create_dir_all` — a **plain directory named
`.worktrees/phase-N`, not a linked `git worktree`**. That is adequate for the unit under test
(`select_loop_back_fix` only stats the filesystem), but it means nothing in the suite exercises
real linked-worktree semantics, and a future implementation that asked `git` instead of the
filesystem would pass these tests while being wrong. I did not build a running DevFlow in worktree
mode and drive a live Validate→Code loop.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01 (BLOCKER): the evidence-root defect is fixed at one call site and still live at another — `evaluate_layer0` reads the external-verify declaration from the main checkout

**File:** `crates/devflow-core/src/agent_result.rs:2025-2042` (reached via `:2301-2304` from
`evaluate_agent_result`, called at `pipeline_launch.rs:883` for **every** stage)

**Issue:** two lines sit next to each other and disagree about which root the phase's `.planning/`
artifacts live in:

```rust
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);   // :2041 — correct
let commands = crate::verify::external_verify_commands(project_root, state.phase); // :2042 — wrong root
```

`external_verify_commands` globs `project_root/.planning/phases/{NN}-*/{NN}-*-PLAN.md`
(`verify.rs:28-37`, `:72`). The doc comment justifying the split at `:2025-2031` states the reason
outright — *"`project_root` is used to DISCOVER the PLAN's declared commands (`.planning/phases/`
lives there, not in a worktree checkout)"* — and that factual claim is false. `.planning/` is a
tracked directory (`git ls-files .planning` is non-empty; the only ignored entry is the
`UPSTREAM-GSD-ISSUES.md` symlink), so a phase's plans are committed on `feature/phase-N` and are
absent from a main checkout on `develop` for the phase's entire in-flight duration. Measured on
this repository, with the control alongside the subject:

```
git ls-tree -r --name-only develop -- .planning/phases | grep -c '33-'   ->  0
git ls-tree -r --name-only HEAD    -- .planning/phases | grep  '33-'     ->  33-01-PLAN.md, 33-01-SUMMARY.md, …
```

This is exactly the reasoning `select_loop_back_fix`'s new doc comment
(`pipeline_outcomes.rs:244-260`) now spells out for the *other* call site. Two files in this
review's own scope assert opposite facts about the same directory.

Consequences in worktree mode — DevFlow's default (`commands::start`,
`commands.rs:238-244`; `external_verify_enabled` defaults to `true`, `config.rs:81`):

1. **Default case (`DEVFLOW_TRUST_EXTERNAL_VERIFY` unset):** `commands` is empty, so `:2044`'s
   `approved_commands.map(...)` yields `None` and Layer 0 is skipped entirely. Declared external
   verification **silently never runs** for any worktree-mode phase. A verification gate that is
   configured, declared in the PLAN, and never executed is a false-assurance defect, not a
   cosmetic one — and it is invisible because the fallthrough to Layer 1/2 looks like a normal
   evaluation.
2. **Trusted case (`DEVFLOW_TRUST_EXTERNAL_VERIFY` set to the reviewed JSON array):** the same
   empty `commands` takes the `:2044` arm and returns
   `Failed { reason: "external verification approval mismatch; PLAN declaration was removed" }`
   — a hard, unconditional false failure on every stage of every worktree-mode phase, routed
   through `handle_stage_failure`'s never-silent gate (or, at Validate, straight into the
   Code↔Validate loop this phase exists to make correct).

**The one reading under which this is not a bug, and why I reject it.** One could argue the main
checkout is deliberate as a *trust boundary*: read the declaration from a root the sandboxed agent
cannot write, and diff it against the operator-approved env value to close the review-to-execution
TOCTOU (`verify.rs:12-20`). Two things defeat that reading. First, `verify.rs:118-121` already
concedes these files "are agent-writable," so the boundary is not being claimed elsewhere in the
module. Second, and decisively, the behavior is broken under that reading too: the plan is *never*
on `develop` while the phase is in flight, so the declaration is not merely less-trusted from the
main checkout — it is absent, every time, and the TOCTOU comparison degrades to the two failure
modes above rather than to a stricter check.

**Fix** (one-line behavioral change plus the doc correction, which must not be skipped — the stale
comment is what will re-introduce this):

```rust
// agent_result.rs, evaluate_layer0
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
// `.planning/` is TRACKED, so a phase's PLAN.md is committed on feature/phase-N and is
// invisible from a main checkout on develop for the phase's entire in-flight duration.
// Discovery must follow the agent's cwd, exactly as `select_loop_back_fix` does
// (pipeline_outcomes.rs) — see that function's CR-01 note.
let commands = crate::verify::external_verify_commands(execution_root, state.phase);
```

and add a regression test with `state.worktree_path = Some(wt)` where the PLAN's declaration
exists **only** under `wt`, mirroring `worktree_mode_genuine_gaps_loop_back_issues_gaps_only`'s
shape, with the main-checkout-only case as its negative control.

**Disposition.** This is pre-existing (it predates the merge base) and fixing the behavior is a
scope expansion for phase 33. But the *doc contradiction* is this phase's to resolve, because this
phase's diff is what makes the two comments provably incompatible: correct
`agent_result.rs:2025-2031` in this phase and file the behavioral fix as a numbered ROADMAP
backlog entry. Shipping the phase with both comments standing leaves the next reader with two
authoritative, opposite statements and no way to tell which one to trust.

**Related, outside the reviewed file set** (same class, same root cause, flagged not fixed):
`verify::phase_has_blocking_human_checkpoint(project_root, phase)` at `pipeline_launch.rs:957`
reads the same `.planning/phases/` glob from the main checkout, so the entire D-01/28-03 checkpoint
auto-decide path can never fire in worktree mode.

## Warnings

### WR-01 (WARNING, carried — STILL OPEN): the forward-progress reset removes the only unconditional bound on the Code↔Validate loop, and the deferral is still untracked

**File:** `crates/devflow-core/src/mode.rs:136-151`; `crates/devflow-cli/src/pipeline_outcomes.rs:334-359`

**Issue:** unchanged since the first review. `consecutive_failures_made_progress` resets the streak
to 1 whenever the commit *count* rises, and no other counter bounds this loop (`infra_failures`,
`preflight_retries`, `checkpoint_resumes` each bound a different shape). `mode.rs:136-148` states
the weakness honestly but defers the remedy to "a follow-up if the assumption proves wrong."

Confirmed still untracked: the only 999.66 entry in `.planning/ROADMAP.md` (line 1755) is the
backlog entry for the defect phase 33 *fixed*, not for the weakness it introduced. There is no
numbered entry naming the deferral, so the follow-up has no home.

The reason this matters more on this repo than in the abstract: the Code stage's fix command is a
GSD command, and GSD commands routinely commit `.planning/` artifacts (SUMMARY.md, STATE.md) even
when they change no source. "Commits something trivial on every cycle" is not an adversarial
hypothetical here — it is the ordinary behavior of the thing occupying that slot.

**Fix:** either (a) a progress-independent per-phase Code↔Validate cycle ceiling, or (b) require
the `develop..feature/phase-NN` range to touch a non-`.planning/` path before counting it as
progress. Whichever is chosen, file it as a numbered ROADMAP backlog item and cite the number from
`mode.rs`'s doc comment.

### WR-02 (WARNING, carried — STILL OPEN): `phase_verification_exists` has no staleness invalidation, and its doc comment still does not say so

**File:** `crates/devflow-core/src/agent_result.rs:2567-2596`

**Issue:** the doc comment in HEAD is byte-identical to 33-01's. Nothing deletes or dates
`{N}-VERIFICATION.md`, so a phase whose plan set grows after a first verification keeps choosing
`GapsOnly` for plans that were never judged.

33-05 gives this a concrete new instance rather than closing it. Now that the probe follows
`state.worktree_path` unfiltered, a `--force` re-run of the same phase number
(`ensure_phase_worktree(project_root, phase, force)`, `commands.rs:239`) checks out
`feature/phase-NN`, which still carries the **previous** run's committed `{N}-VERIFICATION.md`.
The re-run is mid-arc by construction and will be handed `--gaps-only` on its first Validate
failure — the exact unresolvable-gate outcome D-01 exists to prevent, reached from the new
direction the fix opened.

**Fix:** at minimum, record the limitation in the doc comment now. A real fix compares the
artifact's mtime (or a recorded plan count) against the phase's current plan set.

### WR-03 (WARNING, carried — STILL OPEN): a transient `git` failure grants a free counter reset, and the doc comment asserts the opposite

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:280-286` (doc), `:334-358`;
`crates/devflow-core/src/agent_result.rs:1838-1861`

**Issue:** `phase_commit_count` returns `0` indistinguishably for "no commits", "branch missing"
and "git could not be run" — documented at `agent_result.rs:1838-1840`. But
`handle_validate_outcome`'s doc comment at `:284-286` promises the failure direction is toward
gating: *"an unrunnable `git` or a missing branch counts zero every cycle, so once a baseline is
recorded the counter accumulates and the gate stays reachable."* That holds only while git stays
broken. A **single transient** failure writes `Some(0)` as the baseline at `:357`; on the next
cycle git works, reads the branch's real count (say 40), and `40 > 0` reports progress, resetting
`consecutive_failures` to 1 with no new work done. One flaky `git` invocation buys one free ceiling
reset — the opposite of what the comment promises, at the moment the promise matters.

**Fix:** distinguish "counted zero" from "could not count". Add a sibling returning
`Option<u32>` and treat `None` as *not progress* without overwriting the baseline:

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
    // Could not measure: accumulate, and leave the baseline untouched so the next
    // successful measurement compares against the last REAL observation.
    None => state.consecutive_failures = state.consecutive_failures.saturating_add(1),
}
```

If that is out of scope, the doc comment at `:284-286` must be corrected — it currently asserts a
safety property the code does not have.

### WR-04 (WARNING, carried — STILL OPEN): the gate context under-reports the failure count in Supervise mode

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:367-370`, `:344-353`

**Issue:** the gate text interpolates `state.consecutive_failures` into `"Validation failed {}
time(s) — human review needed."`. Before 33-03 that was a genuine within-loop failure count; after
33-03 it is a *streak length*. In Supervise mode — where Validate gates on **every** failure, not
only at the ceiling (`mode.rs:173-175`) — a phase whose Code stage commits anything each cycle
shows `"Validation failed 1 time(s)"` at the 2nd, 5th and 9th gate alike. The human being asked to
adjudicate reads a number that materially understates how long this has been going on. The comment
at `:344-346` shows the message was considered (that is why the reset writes `1`, not `0`) but the
Supervise case was not.

**Fix:** interpolate a separate, never-reset per-phase Validate-failure total (which would also
give WR-01 its ceiling for free), or reword so the number is honest — `"Validation failed {n}
time(s) in the current streak"` plus the commit-count delta that caused the reset.

### WR-05 (WARNING, carried — STILL OPEN and now WORSE): PATH is restored by a trailing statement, not RAII; 33-05 added two more such regions

**File (new instances):** `crates/devflow-cli/src/pipeline_outcomes.rs:1615-1631`, `:1709-1729`
**File (pre-existing instances):** `:1280-1304`, `:1355-1383`, `:1434-1458`, `:1489-1508`,
`:1545-1560`, `:1779-1798`, `:1850-1865`, `:1915-1930`; `crates/devflow-cli/src/pipeline_gate.rs:1140-1159`, `:1231-1246`
**Guard:** `crates/devflow-cli/src/test_support.rs:50`

**Issue:** every region has the shape `lock → set_var → work → set_var(restore)` with the restore
as a plain trailing statement. Rust abandons the remaining statements of a function the instant a
panic begins unwinding, so a panic inside the region (a) leaves `PATH` pointed at
`neutral_path_dir`, which the unwind then drops and *deletes*, giving every other parallel test
thread a PATH naming a nonexistent directory, and (b) poisons `ENV_MUTEX`, so every subsequent
`ENV_MUTEX.lock().unwrap()` panics with `PoisonError` — one legible failure becomes a cascade.

`test_support.rs:344-355` and `:392-398` document this exact mechanism at length for a different
resource and solve it there with `ReapMonitorOnDrop`, whose doc comment says it plainly: *"it is
the language's own `Drop` guarantee — not a call ordering convention — that makes the reap
unconditional."* The PATH regions never got the same treatment, and 33-05 added two more instead of
adopting the guard that already exists two files away.

**Fix:** an RAII guard in `test_support.rs`, modeled on `ReapMonitorOnDrop`, so the restore is
unconditional and the tempdir outlives it:

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
(`.unwrap_or_else(PoisonError::into_inner)`): the mutex guards a `()`, so there is no invariant a
poisoned lock could be protecting.

### WR-06 (WARNING, carried — STILL OPEN): 33-04's spawn hardening is applied to one of the four sites that need it

**File (hardened):** `crates/devflow-cli/src/pipeline_gate.rs:1111-1170`;
`crates/devflow-cli/src/pipeline_outcomes.rs:1823-1881`
**File (not hardened):** `crates/devflow-cli/src/pipeline_outcomes.rs:819-861` (baseline seed at
`:834`, no PATH neutralization), `:873-930` (`drive_validate_advance_and_read_gate_context`,
baseline seed at `:885`, backing three tests), `:2057-2081`
(`consecutive_failures_increment_saturates`, baseline seed at `:2067`)

**Issue:** 33-04 correctly identified that a test seeding `consecutive_failures` directly now
carries a `None` baseline, takes the reset arm, drops off the gated path and falls through to
`loop_back_to_code` → `launch_stage` → a real `claude` spawn during `cargo test`. Two sites got the
full treatment (baseline seed **plus** neutralized PATH plus a branch-pinning assertion). The three
above got only the baseline seed. Each has a post-hoc assertion that would *notice* the drift, but
in each case the agent spawn happens **before** that assertion runs. A post-hoc detector is not a
preventer, and preventing the spawn is the entire point.

**Fix:** wrap the `handle_validate_outcome` / `advance` call in each of the three with the same
`ENV_MUTEX` + neutralized-PATH block (ideally WR-05's `NeutralPath` guard), so a future drift fails
at `ensure_agent_binary` instead of at a spawned CLI. ~6 lines per site, and it makes the invariant
enforced by construction rather than by three authors independently remembering it.

### WR-07 (WARNING, new): the callee still names its parameter `project_root` — the exact mislabeling that produced the original CR-01

**File:** `crates/devflow-core/src/agent_result.rs:2567-2578`

**Issue:** 33-05 renamed the *caller's* parameter to `evidence_root` and wrote 17 lines of comment
explaining the distinction (`pipeline_outcomes.rs:244-261`). The function it calls is unchanged:

```rust
pub fn phase_verification_exists(project_root: &Path, phase: u32) -> bool
```

and its doc comment (`:2567-2577`) never mentions that the root must follow the agent's cwd. This
is a public item in `devflow-core`, so its signature is the contract a future caller reads first.
The original defect was precisely that a parameter named `project_root` invited `project_root` to
be passed; leaving that invitation intact in the callee, while removing it from one caller, fixes
the instance and preserves the class. The same applies to `phase_review_path` at `:2549`, which is
already called with an `evidence_root` at `:2472` while still naming its parameter `project_root`.

**Fix:** rename the parameter to `evidence_root` in both, and add one sentence to
`phase_verification_exists`' doc comment:

```rust
/// `evidence_root` is the root the Validate agent actually wrote to — the phase's worktree
/// when `state.worktree_path` is set, else the project root. `.planning/` is tracked, so in
/// worktree mode the artifact lands on `feature/phase-N` and is invisible from the main
/// checkout for the phase's entire in-flight duration.
pub fn phase_verification_exists(evidence_root: &Path, phase: u32) -> bool
```

### WR-08 (WARNING, new): the evidence-root resolution is triplicated instead of bound once, so a fourth loop-back arm silently re-introduces CR-01

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:322`, `:378`, `:393`

**Issue:** the expression `state.worktree_path.as_deref().unwrap_or(project_root)` is written out
three times, once per Validate loop-back arm. The inline comments explain the *local binding* (the
shared borrow of `state` must end before `loop_back_to_code` takes it mutably) but not why the
resolution itself could not be hoisted — and it can be, by taking an owned value, which sidesteps
the borrow entirely:

```rust
// Once, at the top of handle_validate_outcome. Owned, so it does not hold a borrow of
// `state` across the `&mut state` calls below.
let evidence_root: PathBuf = state
    .worktree_path
    .clone()
    .unwrap_or_else(|| project_root.to_path_buf());
…
let fix = select_loop_back_fix(&evidence_root, state.phase);
```

The risk is concrete, not stylistic: the defect being fixed here *was* a call site passing the
wrong root, three call sites now each independently encode the right one, and the correctness of a
future fourth arm depends on its author noticing the pattern. One binding makes it impossible to
get wrong; three copies make it a matter of attention. (`PathBuf` is already imported at `:34`.)

## Info

### IN-01 (INFO, carried — STILL OPEN): `select_loop_back_fix` still has no direct unit test

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:261-267`

**Issue:** grep confirms the only references are the three call sites and the doc-comment mentions.
The decision is still reachable only through six `ENV_MUTEX`-serialized, PATH-neutralized,
full-`handle_validate_outcome` drives. 33-05 improved the *coverage* of the decision without
reducing the *cost* of exercising it, so every future case added to this predicate pays the same
full-drive price.

**Fix:** a table test over `(worktree_path, artifact_location) → FixType`, with the existing
end-to-end tests retained as the integration proof.

### IN-02 (INFO, carried — STILL OPEN): a resumed pre-999.66 `state.json` reads as a fresh streak with no distinguishing event

**File:** `crates/devflow-core/src/state.rs:71-100`; `crates/devflow-cli/src/pipeline_outcomes.rs:334-353`

**Issue:** `None` means both "genuine first failure" and "state predates this field"
(`state.rs:80-84` acknowledges both meanings). Nothing in `events.jsonl` tells them apart, so an
operator upgrading a binary mid-phase gets no signal that the effective failure budget just widened
by one.

**Fix:** a distinct reason string on the `loop_back` event for the absent-baseline case.

### IN-03 (INFO, carried — STILL OPEN): the branch-name derivation is still duplicated at the site the count was extracted from

**File:** `crates/devflow-core/src/agent_result.rs:1842` and `:1904`

**Issue:** `phase_commit_count`'s doc comment (`:1827-1831`) says the extraction exists because two
copies of the count *"were able to silently diverge."* `evaluate_layer2` still derives
`let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase)` itself for its five error
messages, while `phase_commit_count` derives the same string internally from the same inputs.
They cannot diverge today; the duplication is the same shape the extraction was written to close,
and there are two further copies (`ship_evidence.rs:160`, `test_support.rs:122`).

**Fix:** expose `pub fn phase_branch_name(git_flow: &GitFlowConfig, phase: u32) -> String` and call
it from all four sites.

### IN-04 (INFO, carried — STILL OPEN): `commit_on_feature_branch` leaves the repo checked out on the feature branch

**File:** `crates/devflow-cli/src/test_support.rs:112-137`

**Issue:** the helper does `git checkout {branch}` (or `-b`) and never restores the prior checkout.
Benign for its two current callers, but a future test pairing this fixture with a Validate→Ship
transition would run `DocsUpdate`/`Merge`/`VersionBump` against the feature branch instead of
`develop` and fail somewhere far from this helper. The doc comment describes the `-B`-vs-`checkout`
care taken but not this post-condition.

**Fix:** capture `git rev-parse --abbrev-ref HEAD` at entry and restore at exit, or document the
post-condition explicitly.

### IN-05 (INFO, carried — STILL OPEN): `fix_prompt`'s preamble contradicts what `FullExecute` means

**File:** `crates/devflow-core/src/prompt.rs:297-307`

**Issue:** all three variants share the preamble `"Validation reported issues. Run the fix command
for this loop:"`. For `FullExecute` that is precisely wrong by D-01's own framing, restated in
`FixType::FullExecute`'s own doc comment at `:55-60` — the phase is *mid-arc*, not defective;
validation reported that plans have not been judged yet, not that they failed. The agent receiving
this prompt is told to treat a normal continuation as a defect.

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

### IN-06 (INFO, new): the new negative control packs two independent scenarios into one `#[test]`, so a scenario-A failure hides the anti-both-roots discriminator

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:1665-1746`

**Issue:** `worktree_mode_mid_arc_loop_back_issues_plain_execute` drives scenario A (phase 94,
no artifact anywhere) and scenario B (phase 95, artifact under the main checkout only) with
separate tempdirs and separate `State`s, then asserts A at `:1737` and B at `:1745`. Scenario B is
the *only* case in the entire suite that fails a "probe both roots and OR them" implementation —
the doc comment at `:1690-1697` says so explicitly. Because A's assertion runs first, any failure
in A aborts before B is ever checked, and the suite silently loses its single discriminator for the
most plausible wrong fix.

**Fix:** split into two `#[test]` functions. They already have independent fixtures; only the
`ENV_MUTEX` guard and the PATH region are shared, and both are cheap to duplicate (or free, under
WR-05's `NeutralPath` guard).

### IN-07 (INFO, new): the new tests set `state.stage = Stage::Code` before driving a Validate outcome — a state the production path cannot be in

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:1599`, `:1678`, `:1694`

**Issue:** `handle_validate_outcome` is only reached from `advance` with `state.stage ==
Stage::Validate` (`pipeline_launch.rs:925-939`, `:989-991`). Setting `Stage::Code` first means
`prepare_loop_back_to_code` captures `gate_stage = Code`, cleans up the **Code** gate rather than
the Validate one, and emits `loop_back` with `"from": "Code"`. It does not affect what these tests
assert (`select_loop_back_fix` never reads `state.stage`), and it copies the pre-existing sibling
at `:1483` — but it means neither new test exercises the gate-cleanup interaction, and the
misleading `from` field would confuse anyone reading the emitted events while debugging.

**Fix:** set `Stage::Validate`, matching `ambiguous_gate_loop_back_respects_the_mid_arc_check`
(`:1766`) and `failure_gate_loop_back_respects_the_mid_arc_check` (`:1831`), which already do.

---

_Reviewed: 2026-08-05_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
_Third pass. The prior report's CR-01 is confirmed closed by inspection of all three call sites
plus a discriminating three-case test set. WR-01 through WR-06 and IN-01 through IN-05 are carried
forward unchanged (WR-05 worsened by two new instances). CR-01 (new), WR-07, WR-08 and IN-06/IN-07
are new._
