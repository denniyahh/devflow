# Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles - Pattern Map

**Mapped:** 2026-08-04
**Files analyzed:** 6 (5 modified existing files + inline test additions; no new files/modules)
**Analogs found:** 6 / 6 — every target file already contains its own closest analog inline (this is a same-file, same-module extension phase, not a new-component phase)

**Note on scope:** All line numbers below were re-verified directly against live source in this session (not copied blind from RESEARCH.md), and matched RESEARCH.md's citations exactly — zero drift found in `pipeline_outcomes.rs`, `pipeline_gate.rs`, `mode.rs`, `state.rs`, `agent_result.rs`, `prompt.rs`. Because this phase is a defect-fix inside an existing, tightly-coupled state machine (not new components), "closest analog" for each touched file is predominantly **an existing sibling function/field in the same file**, not a different file. RESEARCH.md's `## Code Examples` and `## D-03 Recommendation` sections already contain the concrete excerpts needed; this document reorganizes them into the planner-facing classification/analog format and adds the exact backward-compat test pattern to copy.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|--------------------|------|-----------|-----------------|---------------|
| `crates/devflow-cli/src/pipeline_outcomes.rs` — `handle_validate_outcome` (999.65 mid-arc check + 999.66 reset-vs-accumulate branch) | service (pipeline state-machine handler) | event-driven (agent-result → state transition) | same function's own existing increment block (`:264-270`) and 3 `FixType::GapsOnly` call sites (`:254-257`, `:283-286`, `:290-293`) | exact — in-place extension of an existing, well-tested function |
| `crates/devflow-core/src/mode.rs` — new pure predicate (e.g. `consecutive_failures_made_progress`) | utility (pure decision function) | transform | `transition_resets_consecutive_failures` (`:87-113`) | exact — same file, same "pure predicate parallel to an existing one" shape |
| `crates/devflow-core/src/state.rs` — new `#[serde(default)]` field (e.g. `last_validate_failure_commit_count: Option<u32>`) | model (persisted state struct) | CRUD (serde round-trip) | `infra_failures` field + doc comment (`:51-60`) and its two tests, `infra_failures_round_trips_through_serde` / `infra_failures_absent_from_json_defaults_to_zero` (`:347-377`) | exact — identical field-addition shape, 4 prior instances to choose from |
| `crates/devflow-core/src/agent_result.rs` — new commit-count helper (999.66) | utility (git-shelling I/O) | file-I/O / event-driven (subprocess `git`) | `evaluate_layer2`'s commit-count block (`:1862-1881`) | exact — literal block to extract/mirror |
| `crates/devflow-core/src/agent_result.rs` — new `phase_verification_exists`/`phase_verification_path` helper (999.65) | utility (filesystem I/O) | file-I/O | `phase_review_path` (`:2525-2541`) | exact — same directory-prefix-scan idiom, one filename substituted |
| `crates/devflow-core/src/prompt.rs` — optional new `FixType` variant + `fix_prompt` arm (Claude's Discretion) | utility (pure prompt-string builder) | transform | `FixType` enum (`:36-41`) + `fix_prompt` match (`:278-286`) | exact — same file, same enum/match-arm shape |
| Tests (colocated, no separate test dir/framework in this workspace) | test | — | `consecutive_failures_reaches_ceiling_across_cycles` (`pipeline_outcomes.rs:1150-1201`); `infra_failures_absent_from_json_defaults_to_zero` (`state.rs:363-377`); `fix_prompts_select_the_right_command` (`prompt.rs:493-497`) | exact |

## Pattern Assignments

### `crates/devflow-cli/src/pipeline_outcomes.rs` (service, event-driven) — both 999.65 and 999.66 land here

**Analog:** the function's own existing code, verified live at `:235-294`.

**Imports pattern** (top of file, unchanged by this phase — no new imports needed for D-01's file-existence check or D-03's commit-count call, since both underlying helpers live in `devflow-core` and this file already imports `agent_result::` and `mode::`):
```rust
use crate::pipeline_gate::{loop_back_to_code, run_gate, run_gate_with_timeout, transition};
use devflow_core::{
    agent_result::{self, AgentStatus, Verdict},
    mode,
    prompt::FixType,
    stage::Stage,
    state::State,
    workflow, CliError,
};
```
(Confirm exact import list at file top when implementing — not re-read in full this session since the existing use of `agent_result::` and `mode::` at call sites below is sufficient evidence both modules are already imported.)

**Core pattern — the 3 in-scope call sites to gate on 999.65's new check** (all `[VERIFIED]` live, unchanged from RESEARCH.md):
```rust
// :246-259 (Ambiguous-gate loop-back arm)
return match run_gate(project_root, state, Stage::Validate, &context)? {
    GateAction::Advance => transition(project_root, state, Stage::Ship),
    GateAction::LoopBack(_) => {
        loop_back_to_code(project_root, state, FixType::GapsOnly)
    }
    GateAction::Abort(reason) => abort(project_root, state, &reason),
};

// :283-286 (consecutive-failure gate loop-back arm)
GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),

// :290-293 (plain Failed tail match — the common auto-loop path)
match result {
    ValidateResult::Passed => transition(project_root, state, Stage::Ship),
    ValidateResult::Failed => loop_back_to_code(project_root, state, FixType::GapsOnly),
}
```
Per D-02/RESEARCH.md's recommendation, replace the bare `FixType::GapsOnly` literal at all three sites with a single small helper call, e.g. `select_loop_back_fix(project_root, state.phase)`, so the D-01 check is expressed once, not duplicated three times.

**The increment site — where 999.66's reset-vs-accumulate branch replaces the unconditional increment**, `[VERIFIED :264-270]`:
```rust
if result == ValidateResult::Failed {
    // Now that the counter genuinely accumulates (18d), an unbounded
    // loop could otherwise overflow it and wrap to 0, silently
    // restoring the unreachable-ceiling bug in a slower form.
    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    workflow::save_state(state)?;
}
```
This is the ONLY site to touch for 999.66 (do not touch `prepare_loop_back_to_code` — see Anti-Pattern below). Becomes: compute current commit count via the new `agent_result` helper (I/O), call the new `mode::` pure predicate with `(state.last_validate_failure_commit_count, current_commits)`, branch reset-vs-`saturating_add`, then unconditionally update `state.last_validate_failure_commit_count = Some(current_commits)` before `save_state`.

**4th call site — confirmed out of scope, do not touch**, `[VERIFIED :319-321]` inside `handle_ship_outcome`:
```rust
match run_gate_with_timeout(...)? {
    GateAction::Advance => finish_workflow(project_root, state),
    GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
    GateAction::Abort(reason) => abort(project_root, state, &reason),
}
```

**Test pattern to extend** (regression guard for criterion 4 — must keep passing unchanged; new criterion-3 variant should closely mirror this shape but commit real work mid-loop), `[VERIFIED :1150-1201]`:
```rust
#[test]
fn consecutive_failures_reaches_ceiling_across_cycles() {
    let _guard = ENV_MUTEX.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = 81;
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    workflow::save_state(&state).unwrap();

    let response_path = Gates::response_path(root, phase, Stage::Validate);
    std::fs::create_dir_all(response_path.parent().unwrap()).unwrap();

    let neutral_path_dir = agent_free_git_only_path_dir();
    // ... PATH neutralized under ENV_MUTEX so no real agent spawns ...

    for _ in 0..mode::MAX_CONSECUTIVE_FAILURES {
        std::fs::write(&response_path, r#"{"approved":false,...}"#).unwrap();
        let _ = handle_validate_outcome(root, &mut state, ValidateOutcome::Failed);
        state.stage = Stage::Code;
        let _ = transition(root, &mut state, Stage::Validate);
    }

    assert_eq!(state.consecutive_failures, mode::MAX_CONSECUTIVE_FAILURES);
    assert!(state.mode.should_gate(Stage::Validate, state.consecutive_failures));
}
```
The new criterion-3 test (healthy multi-wave, no false gate) needs a test-support addition that actually creates commits on the phase's feature branch between cycles — RESEARCH.md's `## Wave 0 Gaps` flags this as new test-support, not something that exists yet (`init_repo` / `agent_free_git_only_path_dir` exist; a "commit N files on `feature/phase-{NN}`" helper does not).

---

### `crates/devflow-core/src/mode.rs` (utility, pure predicate) — new 999.66 reset predicate

**Analog:** `transition_resets_consecutive_failures`, `[VERIFIED :87-113]`:
```rust
/// Whether `transition()` should zero
/// [`crate::state::State::consecutive_failures`] when moving from `from` to
/// `to`.
/// ...
pub fn transition_resets_consecutive_failures(from: Stage, to: Stage) -> bool {
    !matches!((from, to), (Stage::Code, Stage::Validate))
}
```
**Pattern to copy:** a small, `pub`, doc-commented, pure function with a single-line body and its own dedicated unit test(s) placed in the same `#[cfg(test)] mod tests` block. RESEARCH.md's concrete recommended signature:
```rust
pub fn consecutive_failures_made_progress(previous: Option<u32>, current: u32) -> bool {
    previous.is_none_or(|p| current > p)
}
```

**Test pattern to copy**, `[VERIFIED :237-262]`:
```rust
#[test]
fn consecutive_reset_skips_the_code_to_validate_hop() {
    assert!(!transition_resets_consecutive_failures(Stage::Code, Stage::Validate));
}

#[test]
fn consecutive_reset_fires_on_every_other_transition() {
    // Enumerated explicitly (not a negation of the skip case above) so a
    // future Stage variant added to the linear chain doesn't silently
    // fall through un-asserted.
    assert!(transition_resets_consecutive_failures(Stage::Define, Stage::Plan));
    ...
}
```
Apply the same "enumerate explicitly, don't just negate" discipline to the new predicate's tests (e.g. test `None` → true, `Some(5) < 6` → true, `Some(5) == 5` → false, `Some(5) > 3` → false).

---

### `crates/devflow-core/src/state.rs` (model, CRUD/serde) — new field for 999.66

**Analog:** `infra_failures` field + doc comment, `[VERIFIED :51-60]`:
```rust
/// Consecutive infrastructure-class faults (`ResourceKilled`,
/// `AgentUnavailable`) — distinct from [`Self::consecutive_failures`]
/// (D-08, 17-01). Gates at [`crate::mode::MAX_INFRA_FAILURES`]. Any
/// increment (wired in Plan 04) must use `saturating_add` so a
/// long-running stuck loop cannot overflow `u32`. A serde-absent value
/// (older persisted state) defaults to 0. Reset to 0 on every successful
/// stage transition, alongside `consecutive_failures` (CR-01, 17-06 gap
/// closure), so the ceiling bounds a stuck loop, not a phase's lifetime.
#[serde(default)]
pub infra_failures: u32,
```
**Pattern to copy:** doc comment explains (1) what it counts, (2) which ceiling/predicate consults it, (3) the overflow-safety note if relevant, (4) explicit backward-compat behavior for serde-absent values, (5) reset semantics/who resets it. Then `#[serde(default)]` immediately above the field. RESEARCH.md's recommended field:
```rust
/// Commit count on the phase's feature branch observed at the most
/// recent Validate failure (999.66/D-03). Used by
/// `mode::consecutive_failures_made_progress` to distinguish genuine
/// forward progress (Code produced new commits since the last failure —
/// reset `consecutive_failures`) from the same unresolved problem
/// recurring (no new commits — keep accumulating toward
/// `mode::MAX_CONSECUTIVE_FAILURES`). `None` means "no prior failure
/// recorded yet" (including a serde-absent value from state written
/// before this field existed) and is treated as progress by the
/// predicate, so the very first observed failure never mis-accumulates.
#[serde(default)]
pub last_validate_failure_commit_count: Option<u32>,
```
Remember to also add it to `State::new`'s field-initializer list (mirroring `infra_failures: 0,` etc. at `:236-243`, using `None` for this `Option` field).

**Test pattern to copy** (both tests, one round-trip + one absent-defaults), `[VERIFIED :347-377]`:
```rust
#[test]
fn infra_failures_round_trips_through_serde() {
    let mut state = State::new(1, AgentKind::Claude, Mode::Auto, PathBuf::from("/repo"));
    state.infra_failures = 4;
    let json = serde_json::to_string(&state).unwrap();
    assert!(json.contains("infra_failures"), "infra_failures must appear in persisted JSON");
    let loaded: State = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.infra_failures, 4, "infra_failures must round-trip through serde");
}

/// A serde-absent `infra_failures` (older persisted state.json without
/// the field) must default to 0, not fail to deserialize.
#[test]
fn infra_failures_absent_from_json_defaults_to_zero() {
    let json = r#"{
        "stage": "code",
        "phase": 1,
        "agent": "claude",
        "mode": "auto",
        "started_at": "0",
        "project_root": "/repo"
    }"#;
    let loaded: State = serde_json::from_str(json).unwrap();
    assert_eq!(loaded.infra_failures, 0);
}
```
For the new `Option<u32>` field, the absent-defaults assertion is `assert_eq!(loaded.last_validate_failure_commit_count, None)`.

---

### `crates/devflow-core/src/agent_result.rs` (utility, file-I/O + subprocess) — two new helpers, one per defect

**Analog A (999.66 commit-count helper) — the block to extract/mirror**, `[VERIFIED :1862-1881]` (inside `evaluate_layer2`):
```rust
let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase);

// Verify branch exists before counting commits.
let branch_exists = git_command(project_root)
    .args(["rev-parse", "--verify", &branch])
    .output()
    .map(|o| o.status.success())
    .unwrap_or(false);

let commits: u32 = if branch_exists {
    let range = format!("{}..{branch}", git_flow.develop);
    git_command(project_root)
        .args(["rev-list", "--count", &range])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(0)
} else {
    0
};
```
Extract into a small shared helper, e.g. `fn commits_on_branch(project_root: &Path, git_flow: &GitFlowConfig, phase: u32) -> u32`, called from both `evaluate_layer2` and the new 999.66 site (per RESEARCH.md's `## Don't Hand-Roll` — extraction avoids the two counting mechanisms silently diverging). `git_command(project_root)` (not the worktree path) is the correct call target — git worktrees share refs/object DB, confirmed by this being the exact pattern `evaluate_layer2` already relies on at every call site.

**Analog B (999.65 VERIFICATION.md-existence helper) — exact structural precedent**, `[VERIFIED :2525-2541]`:
```rust
fn phase_review_path(project_root: &Path, phase: u32) -> Option<PathBuf> {
    let phases = std::fs::read_dir(project_root.join(".planning/phases")).ok()?;
    let prefix = format!("{phase:02}-");
    for entry in phases.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&prefix))
        {
            let review = entry.path().join(format!("{phase:02}-REVIEW.md"));
            if review.exists() {
                return Some(review);
            }
        }
    }
    None
}
```
Mirror exactly, substituting `{phase:02}-VERIFICATION.md` for `{phase:02}-REVIEW.md`. This is the only production check in the codebase for artifact existence by phase-directory prefix scan — no second idiom exists to compare against, confirmed by grep (only doc-comments mention `VERIFICATION.md` today).

---

### `crates/devflow-core/src/prompt.rs` (utility, pure string builder) — optional new `FixType` variant

**Analog:** `FixType` enum + `fix_prompt`, `[VERIFIED :36-41, :278-286]`:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixType {
    /// Run the GSD audit-fix pipeline over review findings.
    AuditFix,
    /// Re-run execution targeting only the gaps left by validation.
    GapsOnly,
}
...
pub fn fix_prompt(fix_type: FixType, phase: u32) -> String {
    let command = match fix_type {
        FixType::AuditFix => format!("/gsd-audit-fix {phase}"),
        FixType::GapsOnly => format!("/gsd-execute-phase {phase} --gaps-only"),
    };
    format!(
        "Validation reported issues. Run the fix command for this loop:\n\n    {command}\n\n{COMPLETION_PROTOCOL}"
    )
}
```
If Claude's Discretion (CONTEXT.md) chooses a new variant over branching upstream of the call, add a third doc-commented arm, e.g. `FixType::MidArc => format!("/gsd-execute-phase {phase}")` (no `--gaps-only`), matching the existing two-arm match exactly in shape.

**Test pattern to copy**, `[VERIFIED :493-497]`:
```rust
#[test]
fn fix_prompts_select_the_right_command() {
    assert!(fix_prompt(FixType::AuditFix, 11).contains("/gsd-audit-fix 11"));
    assert!(fix_prompt(FixType::GapsOnly, 11).contains("--gaps-only"));
    assert!(fix_prompt(FixType::AuditFix, 11).contains("DEVFLOW_RESULT"));
}
```
A third assertion for the new variant, following the identical `.contains(...)` shape, is the minimal test if a variant is added.

---

### `crates/devflow-cli/src/pipeline_gate.rs` — read-only reference, likely NOT modified

**Analog/context only** — `prepare_loop_back_to_code`, `[VERIFIED :130-158]`:
```rust
pub(crate) fn prepare_loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
) -> Result<String, CliError> {
    let gate_stage = state.stage;
    let _ = Gates::cleanup(project_root, state.phase, gate_stage);
    state.stage = Stage::Code;
    state.gate_pending = false;
    workflow::save_state(state)?;
    events::emit(project_root, state.phase, "loop_back", serde_json::json!({
        "from": gate_stage.to_string(),
        "consecutive_failures": state.consecutive_failures,
    }));
    println!("looping back to Code (validate failures: {})", state.consecutive_failures);
    Ok(prompt::fix_prompt(fix, state.phase))
}
```
Confirmed live: this function only mutates `stage`, `gate_pending`, and cleans up gate files — it never touches `consecutive_failures`. No 999.66 logic belongs here (see Anti-Pattern below). It may still need a one-line change ONLY if a new `FixType` variant requires no further wiring here — `fix` is already an opaque parameter passed straight to `prompt::fix_prompt`, so a new variant needs zero changes to this function's body, only to `prompt.rs`'s match.

## Shared Patterns

### Pure-predicate-consulted-by-I/O-caller (the load-bearing architectural pattern for both defects)
**Source:** `transition_resets_consecutive_failures` (`mode.rs:111-113`), consulted only from `transition()` (`pipeline_gate.rs:95`)
**Apply to:** the new `mode::consecutive_failures_made_progress` predicate — must stay a bare `(Option<u32>, u32) -> bool` function with no `Path`/`git`/I/O in its signature; all I/O (the `git rev-list --count` shell-out, the file-existence check) happens in the CLI-layer caller (`handle_validate_outcome`), and only the derived boolean/count is passed in. RESEARCH.md's `## Anti-Patterns to Avoid` names widening this signature as an explicit anti-pattern.

### `#[serde(default)]` counter-field convention
**Source:** `consecutive_failures`, `infra_failures`, `preflight_retries`, `checkpoint_resumes` (`state.rs:46-113`)
**Apply to:** the new `last_validate_failure_commit_count` field — same attribute, same doc-comment structure (what it counts / what consults it / reset semantics / backward-compat note), same paired round-trip + absent-defaults test.

### Directory-prefix-scan-under-`.planning/phases/` idiom
**Source:** `phase_review_path` (`agent_result.rs:2525-2541`)
**Apply to:** the new `phase_verification_exists`/`phase_verification_path` helper — identical scan shape, only the target filename changes.

### `saturating_add` on every accumulating counter
**Source:** `pipeline_outcomes.rs:264-270`'s own comment: "an unbounded loop could otherwise overflow it and wrap to 0, silently restoring the unreachable-ceiling bug in a slower form"
**Apply to:** `consecutive_failures`'s accumulate branch must keep `saturating_add(1)`, unchanged. The new `last_validate_failure_commit_count` field is *replaced* wholesale each failure (not incremented), so this concern does not apply to it — do not add unnecessary saturating arithmetic there.

## No Analog Found

None — every file this phase touches already has a directly-applicable, same-file or same-crate analog, confirmed above. This phase is scoped entirely within already-shipped, already-patterned code (RESEARCH.md's own framing: "do what this codebase already does elsewhere, not new design").

## Anti-Patterns Flagged by Research (do not treat as "no analog" — these are explicitly wrong analogs to avoid)

- **Do not** add `consecutive_failures` reset/accumulate logic to `prepare_loop_back_to_code` (`pipeline_gate.rs:130`). It fires on every loop-back unconditionally; putting the reset decision there is structurally identical to the forbidden "reset on every loop-back" bug. The fix belongs entirely in `handle_validate_outcome`'s existing increment block (`pipeline_outcomes.rs:264-270`), upstream of `loop_back_to_code`.
- **Do not** reuse the 999.65 `{N}-VERIFICATION.md`-existence boolean as the 999.66 reset signal. `{N}-VERIFICATION.md` is absent for the entire duration of a multi-wave phase until the very end — using it as the reset signal degenerates to "always reset," the same forbidden bug in a different disguise. The two signals must stay computed and threaded separately even though both land in the same function.

## Metadata

**Analog search scope:** `crates/devflow-cli/src/{pipeline_outcomes.rs,pipeline_gate.rs}`, `crates/devflow-core/src/{mode.rs,state.rs,agent_result.rs,prompt.rs}` — the exact 6 files named in CONTEXT.md's canonical refs and RESEARCH.md's sources; no broader codebase search was needed since this is an in-place defect-fix phase with all analogs living in the same files/modules being modified.
**Files scanned:** 6 (all read or targeted-range-read directly this session; all line numbers cross-checked against RESEARCH.md with zero drift found)
**Pattern extraction date:** 2026-08-04
