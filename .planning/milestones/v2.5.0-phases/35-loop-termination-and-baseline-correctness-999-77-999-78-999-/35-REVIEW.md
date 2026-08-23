---
phase: 35-loop-termination-and-baseline-correctness-999-77-999-78-999-
reviewed: 2026-08-07T14:44:45Z
depth: standard
diff_range: 749a151..HEAD
files_reviewed: 11
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/pipeline_gate.rs
  - crates/devflow-cli/src/pipeline_launch.rs
  - crates/devflow-cli/src/pipeline_outcomes.rs
  - crates/devflow-cli/src/test_support.rs
  - crates/devflow-cli/tests/release_check.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/test_support.rs
findings:
  critical: 1
  warning: 7
  info: 3
  total: 11
status: issues_found
---

# Phase 35: Code Review Report

**Reviewed:** 2026-08-07T14:44:45Z
**Depth:** standard (per-file analysis of the `749a151..HEAD` diff, Rust-specific checks)
**Files Reviewed:** 11
**Status:** issues_found

## Summary

The `Option<u32>` migration itself is **complete and correct at every call site** — I traced all
three consumers (`handle_validate_outcome`, `evaluate_layer2`, `evaluate_layer3`) and none of them
collapses `None` to zero; `rg` over the whole workspace finds no surviving `.unwrap_or(0)` on a
`phase_commit_count` result. The two new `State` fields both carry `#[serde(default)]` and both have
absent-key round-trip tests. `phase_validate_failures` has exactly two production writes (one
`saturating_add`, one reset) and one carry-forward, and `abort()` genuinely does call
`clear_state`, so the "no reset on abort" comment is true. The `pre_exec` closure calls only
`libc::setsid()`, which is async-signal-safe; the SAFETY comment is accurate. No `.unwrap()` /
`.expect()` / `panic!` was introduced outside `#[cfg(test)]`.

The problem is **where** the new `None` handling was placed in `evaluate_layer2`. The early return
sits above the exit-code classification rather than beside the one computation that needs a count,
so an unmeasurable count now discards the `ResourceKilled` / `AgentUnavailable` / `Failed` /
`Success` classification for *every* stage and routes the whole cascade to Layer 3. That
reintroduces the exact routing the codebase's own comments (`pipeline_launch.rs:1114-1117`,
`pipeline_outcomes.rs:37-43`) declare forbidden — an infrastructure fault being fed to
`handle_validate_outcome`. It is the one BLOCKER below.

Secondary theme: several of the new operator-facing signals are correct in the code and wrong in
the message. The ceiling reset runs before the line that reports the total, so a run that just hit
the ceiling prints `0 validate failure(s) this phase`; a *passing* Validate at the ceiling fires an
unexplained Auto-mode gate; and a timeout in the signing probe — a measurement failure by the
probe's own taxonomy — is rendered as a hard `fail` verdict about the key.

## Critical Issues

### CR-01: `evaluate_layer2`'s unmeasurable-count fall-through discards the exit-code classification, routing infra faults into the Validate loop

**File:** `crates/devflow-core/src/agent_result.rs:1950-1962`
**Also implicated:** `crates/devflow-cli/src/pipeline_launch.rs:1100-1118`,
`crates/devflow-cli/src/pipeline_outcomes.rs:37-53`

**Issue.** The new guard is placed *before* everything it does not govern:

```rust
let Some(commits) = phase_commit_count(project_root, git_flow, phase) else {
    return Ok(None); // fall to Layer 3
};

let commit_gated = matches!(stage, Stage::Plan | Stage::Code);
let no_work_done = commit_gated && commits == 0;

let status = if exit_code == 137 { ResourceKilled }
             else if exit_code == 127 { AgentUnavailable }
             else if exit_code != 0 || no_work_done { Failed }
             else { Success };
```

`commits` is load-bearing for exactly one term — `no_work_done`, which only exists when
`commit_gated` is true. The 137 / 127 / `exit != 0` arms and the non-commit-gated `Success` arm do
not read it at all (they interpolate it into reason strings, nothing more). By returning early, a
`None` count now throws away all four of those classifications.

Layer 1 cannot cover for this: an agent killed by SIGKILL, or one that never launched, emits no
`DEVFLOW_RESULT` marker and no Claude `result` event, so `evaluate_layer1` returns `None`
(`agent_result.rs:1789-1807`) and **Layer 2 is the sole classifier for exit 137 and 127**. Layer 3
has no `ResourceKilled` or `AgentUnavailable` arm; the best it can produce is `Unknown`.

**Concrete failure scenario (host under memory pressure).** The Code-stage agent is OOM-killed;
the monitor writes `137` to `.devflow/phase-NN-exit`. The same memory pressure makes the `fork` for
`git rev-parse` fail, so `.output()` returns `Err` and `phase_commit_count` returns `None` — this is
not a coincidence, it is one root cause producing both observations.

| | before this phase | after this phase |
|---|---|---|
| Layer 2 result | `ResourceKilled`, `commits: Some(0)` | `Ok(None)` — no result |
| final status | `ResourceKilled` | `Unknown` (Layer 3) |
| `decide_action` | `Action::GateInfra` | `Action::GateReview` |
| handler | `handle_infra_outcome` | `handle_stage_failure` (Code/Plan/Define) or `handle_validate_outcome(ValidateOutcome::Failed)` (Validate) |
| `infra_failures` | `+1`, ceiling → abort | never incremented — the infra ceiling is unreachable |
| `consecutive_failures` | untouched (by design) | `+1` |
| `phase_validate_failures` | untouched | `+1` (Validate) |

The last two rows are the ones that matter. `pipeline_launch.rs:1114-1117` states the invariant
explicitly — *"MUST NOT route through handle_validate_outcome/handle_ship_failure, which would bump
consecutive_failures (review consensus #4, D-08)"* — and this change makes that routing happen.
An OOM-killed Validate agent is now recorded as an agent-caused validation failure and consumes the
per-phase budget this very phase introduced, while `MAX_INFRA_FAILURES` never accumulates and the
infra abort path becomes unreachable for the correlated case.

**Second, independent harm on the same line.** For a non-commit-gated stage (`Define`, `Validate`,
`Ship`) that exited **0**, the documented matrix at `agent_result.rs:1928` says `Success`. With an
unmeasurable count it is now `Unknown` → `GateReview`. For `Stage::Validate` that means a run whose
Validate agent exited cleanly is dispatched as `ValidateOutcome::Failed`. The commit count was never
part of that decision; the fall-through makes it part of it.

**On reachability.** This is not exotic. `phase_commit_count` returns `None` in two situations, and
only one of them is "git is missing": (a) `.output()` returns `Err` — fork/spawn failure, an
inaccessible `project_root` (`hermetic_command` sets `current_dir`), a pruned worktree; and (b)
`git rev-list --count {develop}..{branch}` **runs and exits non-zero**, whose empty stdout fails to
parse. Case (b) is not gated on the command having run — unlike the `rev-parse` step, the
`rev-list` step's exit status is never checked — so any condition that makes the range invalid
(the configured `git_flow.develop` absent from the checkout, a shallow clone) puts the cascade
permanently into this state, and then *every stage of every phase* gates for review.

**Contradicted claim.** `35-01-SUMMARY.md` records under stated limits: *"F-5 (a): no
dispatch-level change. `AgentStatus::Failed` and `AgentStatus::Unknown` map identically to
`Action::GateReview`."* That is true of the Layer 3 edit and false of the Layer 2 edit, which
changes `Success → Unknown` (Advance → GateReview) and `ResourceKilled → Unknown`
(GateInfra → GateReview). The limit as written does not cover what shipped.

**Fix.** Require the count only where it decides something, and let the exit code classify first:

```rust
let commits = phase_commit_count(project_root, git_flow, phase);
let commit_gated = matches!(stage, Stage::Plan | Stage::Code);

// D-09 (999.87): fall to Layer 3 ONLY when the missing count is what would
// have decided — i.e. the commit gate is live and the exit code says nothing.
// 137/127/exit!=0 are classified from the exit code alone and must keep their
// ResourceKilled/AgentUnavailable/Failed verdicts, and a non-commit-gated
// stage that exited 0 was never counting commits in the first place.
if commit_gated && exit_code == 0 && commits.is_none() {
    return Ok(None); // fall to Layer 3
}

let no_work_done = commit_gated && commits == Some(0);
// ...reason strings render `commits` as "unknown" when None rather than a number.
```

Add a regression test pairing the two directions under one forced-`git` failure: exit `137` must
still yield `ResourceKilled` / `Action::GateInfra`, and exit `0` on `Stage::Code` must still fall
through to Layer 3. A test that only asserts the fall-through cannot tell the two apart — which is
how this got through.

## Warnings

### WR-01: A probe timeout is a measurement failure but is rendered as a hard `NotViable` verdict about the key

**File:** `crates/devflow-core/src/git.rs:1085-1096` (the `SignProbeOutcome` → `SigningViability`
mapping), consumed at `crates/devflow-cli/src/commands.rs:2474-2479`

**Issue.** `SignProbeOutcome` has three "we could not establish anything" classes and they are not
treated alike:

| outcome | verdict | `release --check` status |
|---|---|---|
| `ToolMissing` | `Unknown` | `warn` |
| `NotRun` | `Unknown` | `warn` |
| `TimedOut` | **`NotViable`** | **`fail`** + `install_hint: "resolve before attempting the signed release tag"` |

`NotRun`'s own doc comment states the principle — *"Fail-soft: an infrastructure problem is not
evidence about the key"* — and 20d/D-06 (cited throughout this file) says an unavailable tool yields
`Unknown`, never a hard-fail `NotViable`. D-01's justification for the timeout names **a wedged
`ssh-agent` and a stalled PKCS11 provider**: both are infrastructure faults, and both now produce a
verdict that asserts something about the key.

**Concrete failure scenario.** `user.signingkey` points at a FIDO/`sk-ssh-ed25519` public key.
`ssh-keygen -Y sign` requires a physical touch on the token; the probe has `stdin/stdout/stderr`
nulled and has just dropped its controlling terminal via `setsid`, so the "Confirm user presence"
prompt reaches nobody. Ten seconds later the probe is killed and `release --check` reports
`tag-signing viability … fail … the signing probe did not finish within its time limit`, with the
remediation hint, for a key that signs correctly under `git tag -s`. That is a false `NotViable`
blocking a release cut — the same defect class 999.86 exists to remove, produced by the replacement
rather than the predictor. *(The fail-closed/fail-soft inconsistency is proven from the code; the
FIDO instance is reasoned from OpenSSH's touch requirement, not measured here.)*

**Fix.** Map `TimedOut` to `SigningViability::Unknown` with a reason naming the ceiling
(`"cannot verify signing viability — the signing probe did not finish within its time limit"`), so
it lands on `warn` beside the other two non-verdicts. If a hard fail is genuinely wanted for a
timeout, record the decision and say why a timeout is evidence about the key, because the file
currently argues the opposite twice.

### WR-02: `fresh_state_carrying_phase_failures` silently zeroes the per-phase budget on any unreadable state file

**File:** `crates/devflow-cli/src/commands.rs:144-148`

**Issue.**

```rust
if let Ok(persisted) = workflow::load_state(project_root, phase) {
    state.phase_validate_failures = persisted.phase_validate_failures;
}
```

Every `WorkflowError` is discarded identically. `load_state` returns `MissingState` for an absent
file *and* a `serde_json` error for one that exists but does not deserialize (hand-edited,
truncated by a full disk, or written by a future schema). Only the first means "no failures
recorded". The second silently hands the phase a fresh 10-failure budget with no operator signal —
defeating the bound whose entire purpose is to survive a restart.

The doc comment above it makes the enumeration explicitly and is wrong: *"An absent or unreadable
persisted state means zero, which is the correct reading for both cases that produce it: a phase's
genuine first start, and a phase whose completion already cleared the file."* "Unreadable" is a
third case; neither named cause covers it.

**Fix.** Discriminate:

```rust
match workflow::load_state(project_root, phase) {
    Ok(persisted) => state.phase_validate_failures = persisted.phase_validate_failures,
    Err(WorkflowError::MissingState(_)) => {}          // genuine zero
    Err(err) => println!(
        "warning: phase {phase} state could not be read ({err}) — the per-phase \
         Validate-failure budget restarts at zero"
    ),
}
```

### WR-03: The ceiling reset runs before the loop-back is reported, so the ceiling event says `0 validate failure(s) this phase`

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:629-634`, with
`crates/devflow-cli/src/pipeline_gate.rs:180-196`

**Issue.** In the ceiling gate's `LoopBack` arm the order is
`select_loop_back_fix` → `reset_phase_failures_at_ceiling` (sets the total to `0`) →
`loop_back_to_code`. `prepare_loop_back_to_code` then reads the *already-reset* value for both the
event and the console line:

```rust
serde_json::json!({ ..., "phase_validate_failures": state.phase_validate_failures, ... })
println!("looping back to Code ({} validate failure(s) this phase, {} in the current streak)", ...)
```

**Concrete failure scenario.** Auto mode, `MAX_PHASE_VALIDATE_FAILURES` reached on the 10th
Validate failure, streak at 1 because trivial `.planning/` commits keep landing (the exact case
999.78 was written for). The gate fires with the correct ceiling message; the operator loops back;
the very next line printed is `looping back to Code (0 validate failure(s) this phase, 1 in the
current streak)` and `events.jsonl` records `"phase_validate_failures": 0` for that loop-back. The
one event in the stream that should mark the ceiling being hit reports the budget as untouched.
Since WR-04's message-side clause is also absent from the `Passed` arm, `gate_fired`'s context
string is the only surviving trace.

**Fix.** Capture the pre-reset total and report that, or move the reset after `loop_back_to_code`
(both arms already persist afterwards):

```rust
GateAction::LoopBack(_) => {
    let fix = select_loop_back_fix(&evidence_root, state.phase, state);
    let result = loop_back_to_code(project_root, state, fix, loop_back_reason(baseline_absent));
    reset_phase_failures_at_ceiling(state, ceiling_gate);
    if ceiling_gate { workflow::save_state(state)?; }
    result
}
```

### WR-04: A *passing* Validate at or above the ceiling fires an unexplained Auto-mode gate and silently resets the budget

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:581-628`

**Issue.** `ceiling_gate` and `should_gate` are both evaluated regardless of `result`, and
`Mode::should_gate` puts `phase_failure_ceiling_reached(...)` ahead of the per-mode match. But the
ceiling clause is appended **only inside the `ValidateResult::Failed` arm** (line 610). So:

**Concrete failure scenario.** Auto mode; `phase_validate_failures == 10` from earlier cycles; the
Code fix finally lands and Validate **passes**. No increment happens, `ceiling_gate` is still
`true`, `should_gate` returns `true`, and the run blocks on a gate whose entire context is
`"Validation passed — approve to ship?"`. An operator running unattended-Auto sees a gate that
this mode is not supposed to fire on a pass, with nothing in the message explaining why.
Answering `Advance` calls `reset_phase_failures_at_ceiling(state, true)`, so the accumulated total
is wiped by a gate the operator never understood to be a ceiling gate.

**Fix.** Either append the ceiling clause in both arms:

```rust
ValidateResult::Passed => {
    let mut message = "Validation passed — approve to ship?".to_string();
    if ceiling_gate {
        message.push_str(&format!(
            " (this phase has recorded {} Validate failures, at the per-phase ceiling of {} — \
             that is why this gate fired in Auto mode.)",
            state.phase_validate_failures, mode::MAX_PHASE_VALIDATE_FAILURES
        ));
    }
    message
}
```

…or scope the ceiling gate to failures, which is what D-07 describes ("*exhausting* the total fires
a human gate"). Pick one deliberately — the current code does neither.

### WR-05: `last_verification_fingerprint` defaulting to `None` on an upgraded state file reproduces the 999.79 defect for every in-flight phase

**File:** `crates/devflow-core/src/state.rs:141-153`, with
`crates/devflow-cli/src/pipeline_outcomes.rs:394-401`

**Issue.** The field's doc comment says:

> `None` means no artifact was observed at the start of this run — the ordinary case for a phase
> being executed for the first time, and also what state written by a binary predating this field
> deserializes to, **which is the same reading**.

It is not the same reading. `verification_authored_this_run(Some(_), None)` returns `true`. For a
phase that was started under 2.4.0 and is continued by a 2.5.0 binary, the previous run's committed
`{N}-VERIFICATION.md` is already on disk while the baseline reads `None` — so the first Validate
failure after the upgrade classifies an inherited artifact as authored-this-run and dispatches
`FixType::GapsOnly` against zero matching plans, gating unresolvably. That is verbatim the
DOGFOOD-01-class stall 999.79 exists to close.

Note the asymmetry with the sibling field added in the same phase: `phase_validate_failures`'s
serde-absent widening was considered important enough to get IN-02's dedicated
`ValidateFailureNoBaseline` loop-back reason so an operator sees the signal. This field, whose
absent case produces a *wrong dispatch* rather than a wider budget, has no equivalent signal.

**Fix.** Either capture the baseline lazily on first use when it is `None` and the run is known to
be mid-arc, or — cheaper and consistent with the existing pattern — emit a distinct loop-back
reason (`gaps_only_without_run_baseline`) when `current.is_some() && baseline.is_none()`, and
correct the doc comment to state the upgrade case as the third, differently-behaving reading it
actually is.

### WR-06: The freshness rule's *false-stale* direction is unguarded — an idempotent Validate rewrite reads as inherited

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:394-401`

**Issue.** `verification_authored_this_run` returns `false` whenever `now == baseline`. Within a
single run the baseline is advanced on every fresh observation (line 357), so a Validate agent that
re-authors `{N}-VERIFICATION.md` with **byte-identical** content on a subsequent failing cycle is
classified as inherited and dispatched `FullExecute`.

**Concrete failure scenario.** Run A, cycle 1: Validate fails and writes a VERIFICATION.md with no
timestamp listing gap G. `current = Some(h)`, `baseline = None` → `GapsOnly`, baseline := `Some(h)`.
Cycle 2: the gaps-only fix did not close G, Validate re-runs and writes the identical document.
`current = Some(h) == baseline` → `FullExecute`. Every subsequent cycle re-runs every plan in the
phase instead of the gaps-only pass Phase 33 built.

This is **not** the already-filed 999.89. That entry records the *false-fresh* direction ("cannot
tell 'this run's agent wrote it' from 'the bytes changed'"). This is the opposite direction — the
bytes did **not** change and the agent *did* write it — and it is the "too strict" failure the
function's own comment (`pipeline_outcomes.rs:336-339`) claims the design guards against.

**Fix.** The predicate needs a second input that distinguishes "unchanged because inherited" from
"unchanged because idempotent" — e.g. record the mtime alongside the fingerprint and treat
`same-hash + mtime-advanced-past-run-start` as authored, or have the Validate stage set a per-cycle
marker in `State` that the selector consults. At minimum, add the missing test: today's pair covers
`stale → FullExecute` and `newly-written → GapsOnly`, and neither exercises a same-run identical
rewrite.

### WR-07: The signing probe's workspace is neither private nor panic-safe, contrary to its own doc comment

**File:** `crates/devflow-core/src/git.rs:906-924`

**Issue.** Two smaller gaps between the comment and the code:

1. *"Creates a **private** workspace"* — `std::fs::create_dir` applies the default mode
   (`0777 & ~umask`, typically `0755`) inside `std::env::temp_dir()`. The directory is
   world-readable and world-traversable. Nothing secret lands in it (the payload is fixed bytes and
   `payload.sig` is a signature over those bytes), so this is not an exposure of key material — but
   "private" is claimed and not implemented, and a future author extending the probe will read the
   comment rather than the mode bits.
2. *"removes the workspace on **every** exit path"* — cleanup is a plain statement in
   `run_ssh_sign_probe`, not a `Drop` guard:

   ```rust
   let outcome = sign_probe_within(&workspace, key_path);
   let _ = std::fs::remove_dir_all(&workspace);
   ```

   It covers every `return` inside `sign_probe_within`, which is what the comment was written for,
   but a panic anywhere in that function (or a kill during the up-to-10-second wait) leaves the
   directory behind. Repeated over many `release --check` runs on a long-lived host, that is
   unbounded accumulation of `devflow-sign-probe-*` directories in `/tmp`.

**Fix.**

```rust
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

let mut builder = std::fs::DirBuilder::new();
builder.mode(0o700);                     // and keep it non-recursive (T-35-12)
if builder.create(&workspace).is_err() { return SignProbeOutcome::NotRun; }

struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}
let _cleanup = Workspace(workspace.clone());
sign_probe_within(&workspace, key_path)
```

## Info

### IN-01: `evaluate_layer2`'s decision matrix was not updated for the new fall-through

**File:** `crates/devflow-core/src/agent_result.rs:1922-1931`

The matrix still enumerates only `exit unknown → fall to Layer 3 (return None)`. The new
"commit count unmeasurable → fall to Layer 3" row is documented three paragraphs away, at the guard
itself, but not in the table a reader consults for the function's contract — and the row
`exit=0, stage NOT in {Plan, Code} … → Success` is now false whenever the count is unmeasurable
(see CR-01). This phase treated doc-comment accuracy as a deliverable elsewhere
(`phase_commit_count`, `handle_validate_outcome`); the same standard applies here.

**Fix:** add the row, and qualify the non-commit-gated `Success` row, in the same change that fixes
CR-01.

### IN-02: `handle_validate_outcome` hardcodes `GitFlowConfig::default()` rather than the project's configured git-flow

**File:** `crates/devflow-cli/src/pipeline_outcomes.rs:523`

Pre-existing (the line was moved, not introduced), but it interacts with CR-01's second reachability
path: if a project configures a non-default `develop` or `feature_prefix`, this call counts against
the wrong branch names, and the resulting `rev-list` failure becomes a permanent `None`. Worth a
`999.x` entry rather than an in-phase fix, per 34/D-04.

### IN-03: `35-01-SUMMARY.md`'s "no dispatch-level change" limit is inaccurate as shipped

**File:** `.planning/phases/35-…/35-01-SUMMARY.md` ("Stated Limits", F-5 (a))

The claim covers the Layer 3 `Failed → Unknown` edit (correct — both map to `GateReview`) but is
presented as covering the plan's dispatch impact generally. The Layer 2 edit changes
`Success → Unknown` (`Advance → GateReview`) and `ResourceKilled → Unknown`
(`GateInfra → GateReview`). Given this repository's standard on stating what a check does *not*
establish, the limit should be corrected alongside the CR-01 fix rather than left as the record.

---

_Reviewed: 2026-08-07T14:44:45Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
_Scope: `git diff 749a151..HEAD` — source files only; `.planning/`, `CHANGELOG.md` and `CLAUDE.md` excluded_
