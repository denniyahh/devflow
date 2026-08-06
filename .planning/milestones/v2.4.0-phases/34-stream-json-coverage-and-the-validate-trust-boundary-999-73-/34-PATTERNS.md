# Phase 34: Stream-JSON Coverage and the Validate Trust Boundary - Pattern Map

**Mapped:** 2026-08-05
**Files analyzed:** 5 source files under change + 1 evidence-directory precedent (no genuinely new
files — this phase is almost entirely modification of existing Rust source and `#[cfg(test)] mod
tests` blocks)
**Analogs found:** 5 / 5 (all in-repo, self-referential — the "analog" for a file is usually that
same file's own established convention)

## File Classification

| File to modify | Role | Data Flow | Closest Analog | Match Quality |
|-----------------|------|-----------|-----------------|---------------|
| `crates/devflow-cli/src/pipeline_launch.rs` (`STREAM_JSON_STAGES` const + its doc comment, `canary_gate_only_applies_to_the_stream_launch_path` test) | config / const + test | request-response (launch-shape selection) | itself — the doc comment on `STREAM_JSON_STAGES` (lines 439-446) is the precedent for recording a widened stage's evidence reason | exact (self) |
| `crates/devflow-cli/src/pipeline_outcomes.rs` (`classify_validate_outcome` rewrite + new 42-cell sweep tests) | service (pure classifier) + test | transform (state-classification) | `crates/devflow-core/src/outcome_policy.rs` `decide_action` (wildcard-free exhaustive match precedent); same file's own test module for AgentResult-literal-construction convention | exact (structural precedent) + exact (self, tests) |
| `crates/devflow-core/src/agent_result.rs` (`reconcile_layer0_verdict` fix + extend `layer0_affirmative_success_consults_layer1_verdict_at_validate` + new worktree-vs-main-checkout fixture) | service (pure transform + cascade) + test | transform / file-I/O (tempdir-backed cascade) | itself — `layer0_affirmative_success_consults_layer1_verdict_at_validate` (5488-5543) and `external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree` (5269-5335) are the direct fixture templates | exact (self) |
| `crates/devflow-core/src/verify.rs` (`phase_has_blocking_human_checkpoint` execution-root fix + new test) | service (pure scan) + test | file-I/O (directory scan) | itself — the `phase_has_blocking_human_checkpoint_*` test family (248-314) and `write_phase_file` helper (242-246) | exact (self) |
| `.planning/phases/34-.../{unit}-evidence/` (new evidence directories, not source) | evidence artifact directory | file-I/O (capture landing spot) | `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/{30c,30c-scrubbed,30c-operator,30a}-evidence/` | exact |

## Pattern Assignments

### `crates/devflow-cli/src/pipeline_launch.rs` — widening `STREAM_JSON_STAGES`

**Analog:** itself, `pipeline_launch.rs:439-480` (read this session, verbatim)

**The doc-comment-records-the-reason pattern** (`pipeline_launch.rs:439-446`):
```rust
/// The stages the Claude `stream-json` launch has been widened to.
///
/// `Stage::Code` first (D-10): it is where 999.64 was observed — Phase 29 wave
/// 2 dispatched two executors from Code and orphaned both — and it is the
/// stage that actually backgrounds work, so it is the only one that exercises
/// task-notification delivery and the drain gate at all. Define would have
/// been a proxy measurement.
const STREAM_JSON_STAGES: &[Stage] = &[Stage::Code];
```
Every future entry in this list must carry the same shape: which decision authorized it (`D-NN`),
what evidence justified it, and — per D-02's "un-widened stage carries a recorded reason" rule —
a stage left OUT must get the same doc-comment treatment naming why (e.g. "capture not obtainable
in this phase; filed as 999.x").

**The predicate this const feeds, and its own doc comment as the pattern for D-03's "one predicate
governs everything" principle** (`pipeline_launch.rs:448-480`):
```rust
fn claude_stream_launch_enabled(agent: AgentKind, stage: Stage, legacy_opt_out: bool) -> bool {
    !legacy_opt_out && agent == AgentKind::Claude && STREAM_JSON_STAGES.contains(&stage)
}
```
Do not add a second predicate or a per-stage env var alongside this one (D-03 forbids it) — widen
only the const and let this function's existing logic pick it up.

**Test needing a rebuild, not a rewrite of intent:** `canary_gate_only_applies_to_the_stream_launch_path`
(`pipeline_launch.rs:1754-1780`, quoted in RESEARCH.md) currently asserts `Stage::Plan` resolves to
the legacy path — that assertion breaks once Plan is widened. Per RESEARCH.md's own recommendation,
rebuild it on the `legacy_opt_out` discriminator (still returns `false` regardless of stage
membership) rather than switching to a non-Claude agent, which would silently change what property
the test demonstrates.

---

### `crates/devflow-cli/src/pipeline_outcomes.rs` — the exhaustive match rewrite (D-06) and the 42-cell sweep (D-08)

**Structural analog for the wildcard-free match:** `crates/devflow-core/src/outcome_policy.rs`
`decide_action` (`outcome_policy.rs:1-67`, read verbatim this session) — the module doc comment
states the pattern this phase's D-06 explicitly continues:

```rust
//! [`decide_action`] is the single exhaustive policy surface `advance()`
//! (Plan 04) dispatches on. It has no I/O, no `CliError`, no filesystem, and
//! no process spawn — deterministic pure function of `(Stage, AgentStatus)`.
//! The `match` has NO wildcard arm: adding a future [`crate::agent_result::AgentStatus`]
//! variant without extending this match is a compile error, which is the
//! mechanism that prevents the D-01 regression class (a new/unhandled
//! outcome silently advancing).
```

The match body itself (`outcome_policy.rs:38-66`) shows how this repo handles arms it does not
want to distinguish — **named, not collapsed with `_`**, with an inline comment explaining why two
variants share a destination:

```rust
pub fn decide_action(_stage: Stage, outcome: AgentStatus) -> Action {
    match outcome {
        AgentStatus::Success => Action::Advance,
        AgentStatus::RateLimited => Action::AutoResume,
        AgentStatus::ResourceKilled => Action::GateInfra,
        AgentStatus::AgentUnavailable => Action::GateInfra,
        // DEFERRED (Plan 01 MEDIUM, OpenCode): Failed and Unknown map
        // identically to GateReview. Intentional — both are non-advance
        // outcomes today and the current phase needs no behavioral
        // distinction between them. The distinction is NOT lost: ...
        AgentStatus::Failed => Action::GateReview,
        AgentStatus::Unknown => Action::GateReview,
        // 31-02 (D-06/D-08). GateReview, not GateInfra: ...
        AgentStatus::IdleTimeout => Action::GateReview,
    }
}
```
Note: `_stage: Stage` (a whole unused *parameter*, not a match position) is fine to ignore
positionally — the ban is specifically on `_` inside the `match` arms over `AgentStatus`. This is
the concrete precedent for D-06's rule: "`_` in the layer or verdict position is fine; only the
status position must be enumerated" — `decide_action` already treats each `AgentStatus` variant by
name (even when two variants share a destination) and treats `_stage` as a don't-care parameter
outside the match. `RateLimited`/`AgentUnavailable`'s decided destination in the new match
(`Failed`, per RESEARCH.md) should carry the same kind of inline comment naming why, referencing
that `decide_action` already routes `RateLimited` to `AutoResume` and the classifier's cell is
unreachable but must still be decided consistently.

**The current implementation being replaced** (`pipeline_outcomes.rs:203-215`, read verbatim):
```rust
pub(crate) fn classify_validate_outcome(result: &agent_result::AgentResult) -> ValidateOutcome {
    let external = result.decided_by_layer == Some(0) && result.status == AgentStatus::Success;
    match (external, result.verdict) {
        (_, Some(Verdict::Pass)) => ValidateOutcome::Passed,
        (true, Some(Verdict::Gaps)) => ValidateOutcome::Ambiguous(
            "external verification passed but the agent reported gaps".to_string(),
        ),
        (true, None) => ValidateOutcome::Ambiguous(
            "external verification passed but no agent verdict arrived".to_string(),
        ),
        _ => ValidateOutcome::Failed,
    }
}
```

**Test-naming and AgentResult-literal-construction convention** — three back-to-back tests in this
same file's test module (`pipeline_outcomes.rs:1009-1130`, read verbatim this session) are the
direct template for the new 42-cell sweep's named-control tests. Naming pattern: a full descriptive
sentence as the function name (`external_verify_agreement_advances_to_ship`,
`external_verify_disagreement_gates_immediately`, `external_verify_no_verdict_gates_immediately`),
each preceded by a doc comment naming the decision it pins (`D-18e`) and what specifically it
asserts:

```rust
/// D-18e's "two independent signals agreeing" arm: a probe pass plus an
/// explicit `verdict: pass` classify as `ValidateOutcome::Passed` and
/// drive straight through to Ship — no forced gate (Auto mode,
/// `consecutive_failures == 0`), no counter touched. ...
#[test]
fn external_verify_agreement_advances_to_ship() {
    let _guard = ENV_MUTEX.lock().unwrap();

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let phase = 90;
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Validate;
    workflow::save_state(&state).unwrap();

    let result = agent_result::AgentResult {
        status: AgentStatus::Success,
        exit_code: None,
        reason: None,
        commits: None,
        summary: None,
        verdict: Some(Verdict::Pass),
        decided_by_layer: Some(0),
    };
    let outcome = classify_validate_outcome(&result);
    assert_eq!(outcome, ValidateOutcome::Passed);
    ...
```

**This is the exact `decided_by_layer` trap D-08/RESEARCH.md's Assumptions section warns about**:
every one of these three existing literals sets `decided_by_layer: Some(0)` explicitly — never
omitted. Any new 42-cell sweep fixture must do the same for every cell meant to represent Layer 0
(`layer0 = true`); a literal that omits the field silently defaults to `None` (per its own doc
comment, `agent_result.rs:38-42`: *"`None` is reserved for test-only fixture literals that don't
route through the real cascade."*) and collapses the whole `layer0 = true` half of the matrix to
untested.

**Simpler literals for the disagreement/no-verdict controls** (`pipeline_outcomes.rs:1072-1080`,
`:1116-1124`) — same shape, only `verdict` and (implicitly) the assertion differ; reuse this
struct-literal style, not a builder or a `Default::default()` shortcut, for the sweep's 42 cells.

**Imports convention** (`pipeline_outcomes.rs:1-30`, module-level, read verbatim):
```rust
use crate::CliError;
use crate::config_parse::{checkout_lock_timeout, gate_timeout_secs};
use crate::parallel::retry_after_from_reason;
use crate::pipeline_gate::{
    abort, finish_workflow, loop_back_to_code, run_gate, run_gate_with_timeout, transition,
};
use crate::pipeline_launch::launch_stage;
use devflow_core::config::GitFlowConfig;
use devflow_core::gates::{GateAction, GateResponse, Gates};
use devflow_core::hooks::{self, HookContext};
use devflow_core::mode;
use devflow_core::prompt::FixType;
use devflow_core::stage::Stage;
use devflow_core::state::State;
use devflow_core::{
    agent_result,
```
Crate-internal imports (`crate::...`) grouped above cross-crate imports (`devflow_core::...`),
each import line resolving to the specific item(s) used, no glob imports, no path aliases.

---

### `crates/devflow-core/src/agent_result.rs` — the `reconcile_layer0_verdict` graft fix (D-15)

**The exact defect being fixed** (`agent_result.rs:2143-2156`, verbatim, per CONTEXT.md/RESEARCH.md
citation, re-confirmed this session at the neighboring test read):
```rust
fn reconcile_layer0_verdict(
    project_root: &Path,
    state: &State,
    result: AgentResult,
) -> AgentResult {
    if state.stage != Stage::Validate
        || result.status != AgentStatus::Success
        || result.decided_by_layer != Some(0)
    {
        return result;
    }
    let verdict = evaluate_layer1(project_root, state.phase).and_then(|layer1| layer1.verdict);
    AgentResult { verdict, ..result }
}
```
The fix (per RESEARCH.md's "Code Examples" section, illustrative not locked) gates the graft on
`layer1.status == AgentStatus::Success` as well as `layer1.verdict` before transplanting.

**Existing test to extend — the direct template, read verbatim this session**
(`agent_result.rs:5484-5543`):
```rust
/// D-05/18e: Layer 0's affirmative-success arm at `Stage::Validate` must
/// consult Layer 1's verdict rather than discard it — the two-signal
/// reconciliation `reconcile_layer0_verdict` adds. Covers all three
/// verdict states Layer 1 can produce: pass, gaps, and no marker at all.
#[test]
fn layer0_affirmative_success_consults_layer1_verdict_at_validate() {
    let dir = tempfile::tempdir().unwrap();
    let phase_dir = dir.path().join(".planning/phases/16-reliability");
    std::fs::create_dir_all(&phase_dir).unwrap();
    std::fs::write(
        phase_dir.join("16-01-PLAN.md"),
        "---\nexternal_verify: \"test -f shipped\"\n---\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("shipped"), "done").unwrap();
    std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
    let mut state = state_in(dir.path(), 16);
    state.stage = Stage::Validate;
    let approval = vec!["test -f shipped".to_string()];

    std::fs::write(
        stdout_path(dir.path(), 16),
        "DEVFLOW_RESULT: {\"status\":\"success\",\"verdict\":\"pass\"}\n",
    )
    .unwrap();
    let result = evaluate_agent_result_inner(
        dir.path(),
        &state,
        &GitFlowConfig::default(),
        Some(&approval),
    )
    .unwrap();
    assert_eq!(result.status, AgentStatus::Success);
    assert_eq!(result.decided_by_layer, Some(0));
    assert_eq!(result.verdict, Some(Verdict::Pass));

    std::fs::write(
        stdout_path(dir.path(), 16),
        "DEVFLOW_RESULT: {\"status\":\"success\",\"verdict\":\"gaps\"}\n",
    )
    .unwrap();
    let result = evaluate_agent_result_inner(
        dir.path(),
        &state,
        &GitFlowConfig::default(),
        Some(&approval),
    )
    .unwrap();
    assert_eq!(result.verdict, Some(Verdict::Gaps));

    std::fs::remove_file(stdout_path(dir.path(), 16)).unwrap();
    let result = evaluate_agent_result_inner(
        dir.path(),
        &state,
        &GitFlowConfig::default(),
        Some(&approval),
    )
    .unwrap();
    assert_eq!(result.verdict, None);
}
```
**The fourth case D-15 needs is one more `std::fs::write(stdout_path(...), ...)` +
`evaluate_agent_result_inner(...)` + `assert_eq!` block appended in the same style**, using the
marker `{"status":"failed","verdict":"pass"}` and asserting `result.verdict == Some(Pass)` pre-fix /
`None` post-fix — reuses `state`, `dir`, and `approval` already in scope, no new fixture needed.

**`state_in` helper — the shared test-state constructor** (`agent_result.rs:2662-2666`, verbatim):
```rust
fn state_in(root: &Path, phase: u32) -> State {
    let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
    state.stage = Stage::Code;
    state
}
```
Callers overwrite `.stage` (as the layer0 test does: `state.stage = Stage::Validate;`) rather than
adding stage as a parameter — follow this pattern for any new fixture that needs a specific stage.

**The 999.76 worktree-vs-main-checkout fixture — the exact template a companion test must mirror**
(`agent_result.rs:5268-5335`, verbatim, doc comment included):
```rust
/// D-05 gap 1 / D-06 (17-03): Layer 0 now evaluates on every stage, not
/// only Code. Also covers the review-flagged worktree bug (Plan 03
/// MEDIUM, OpenCode): PLAN discovery must read `project_root` (where
/// `.planning/phases/` actually lives), while probe execution still
/// reads `execution_root` (the worktree) — using the worktree for
/// discovery would find zero commands and mis-fire the "PLAN removed"
/// veto.
#[test]
fn external_probe_discovers_from_project_root_across_every_stage_and_executes_in_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("phase-worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    let phase_dir = dir.path().join(".planning/phases/16-reliability");
    std::fs::create_dir_all(&phase_dir).unwrap();
    std::fs::write(
        phase_dir.join("16-01-PLAN.md"),
        "---\nexternal_verify: \"test -f implemented\"\n---\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join(".devflow")).unwrap();
    std::fs::write(
        stdout_path(dir.path(), 16),
        "DEVFLOW_RESULT: {\"status\":\"success\"}\n",
    )
    .unwrap();
    let mut state = state_in(dir.path(), 16);
    state.worktree_path = Some(worktree.clone());
    state.stage = Stage::Plan;

    let approval = vec!["test -f implemented".to_string()];

    // Layer 0 now fires on Plan too — the probe file does not yet exist
    // in the worktree, so this must fail on the probe itself (NOT a
    // false PLAN-removed veto, which would mean discovery silently
    // returned zero commands).
    let plan_result = evaluate_agent_result_inner(
        dir.path(),
        &state,
        &GitFlowConfig::default(),
        Some(&approval),
    )
    .unwrap();
    assert_eq!(plan_result.status, AgentStatus::Failed);
    assert!(
        plan_result
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("external verification failed")),
        "expected a failing-probe reason, not a false PLAN-removed veto: {:?}",
        plan_result.reason
    );

    state.stage = Stage::Code;
    let code_result = evaluate_agent_result_inner(
        dir.path(),
        &state,
        &GitFlowConfig::default(),
        Some(&approval),
    )
    .unwrap();
    assert_eq!(code_result.status, AgentStatus::Failed);

    // The probe still executes against execution_root (the worktree) —
    // only PLAN discovery moved to project_root.
    std::fs::write(worktree.join("implemented"), "done").unwrap();
    let passing = evaluate_agent_result_inner(
        dir.path(),
        &state,
        &GitFlowConfig::default(),
        Some(&approval),
    )
    .unwrap();
    assert_eq!(passing.status, AgentStatus::Success);
    assert_eq!(passing.decided_by_layer, Some(0));
}
```

**Note the layout this fixture manufactures — and why it structurally CANNOT catch 999.76's bug**
(per RESEARCH.md's own analysis, confirmed by reading the fixture): the PLAN file is written under
`dir.path()` — which stands in for `project_root` — while `state.worktree_path` points at an EMPTY
sibling directory (`worktree`, created but never given a `.planning/phases/` tree). This fixture
therefore only proves discovery reads `project_root` correctly (which it already does) and that
probe *execution* correctly uses `execution_root`/worktree (the `std::fs::write(worktree.join(...))`
at the end).

**999.76's companion fixture must invert this layout**: write the `.planning/phases/16-reliability/16-01-PLAN.md`
under the `worktree` directory instead of `dir.path()`, leave `dir.path()`'s own `.planning/phases/`
absent or empty, set `state.worktree_path = Some(worktree.clone())`, and assert discovery now
correctly finds the PLAN — using `project_root` still for the top-level `evaluate_agent_result_inner(dir.path(), ...)`
call (unchanged), since 999.76's fix relocates *which root discovery reads*, not which root the test
harness passes as `project_root`. Reuse every other line of the existing fixture verbatim
(`tempdir`, `stdout_path`, `state_in`, the approval vector, the `evaluate_agent_result_inner` call
shape) — only the PLAN file's write-location and the assertion direction change.

**One thing this fix must NOT touch** — `phase_commit_count`'s doc comment
(cited by RESEARCH.md at `agent_result.rs:1832` area, not independently re-read this session beyond
the citation) asserts a deliberately different, correct asymmetry ("must be called with the main
`project_root`, never a worktree path"). Do not retarget that function's root argument when fixing
`evaluate_layer0`'s discovery call.

---

### `crates/devflow-core/src/verify.rs` — the second 999.76 call site (`phase_has_blocking_human_checkpoint`)

**The function being fixed** (`verify.rs:110-127`, verbatim, doc comment included):
```rust
/// Return `true` if any plan declared for `phase` carries a task with the
/// human-blocking checkpoint gate attribute.
///
/// Reads declared plan content only — never any runtime capture or agent
/// output (D-02: no re-implementation of "what does the agent mean"). This
/// is the PRIMARY gate for the auto-decide path added in plan 28-03: ...
pub fn phase_has_blocking_human_checkpoint(project_root: &Path, phase: u32) -> bool {
    const HUMAN_BLOCKING_GATE: &str = r#"gate="blocking-human""#;
    phase_plan_files(project_root, phase)
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|contents| contents.contains(HUMAN_BLOCKING_GATE))
}
```
It delegates discovery to `phase_plan_files(project_root, phase)` (`verify.rs:36-64`) — the SAME
shared discovery function `external_verify_commands` uses and the one 999.76's fix in
`agent_result.rs` must parallel: both currently hardcode `project_root`, never consulting an
execution-root-equivalent value. Fixing this call site means threading an execution-root argument
through here too (`pipeline_launch.rs:957` is the caller, per RESEARCH.md).

**Test-naming and fixture-helper convention** — the `phase_has_blocking_human_checkpoint_*` family
(`verify.rs:242-314`, read verbatim this session) is the direct template for the new worktree-aware
test:
```rust
fn write_phase_file(root: &std::path::Path, phase_dir: &str, file_name: &str, contents: &str) {
    let dir = root.join(".planning/phases").join(phase_dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file_name), contents).unwrap();
}

#[test]
fn phase_has_blocking_human_checkpoint_detects_declared_gate() {
    let dir = tempfile::tempdir().unwrap();
    let body = format!(
        "---\nphase: 91\n---\n\n<task type=\"checkpoint:human-verify\" gate=\"{HUMAN_GATE_VALUE}\">\n</task>\n"
    );
    write_phase_file(dir.path(), "91-probe", "91-01-PLAN.md", &body);

    assert!(phase_has_blocking_human_checkpoint(dir.path(), 91));
}
```
Naming convention: `{function_name}_{condition_under_test}` (e.g.
`phase_has_blocking_human_checkpoint_false_for_plain_blocking_gate`,
`phase_has_blocking_human_checkpoint_true_when_only_second_plan_carries_attribute`), each a single
short assertion, most with an inline string literal explaining WHY the assertion holds (e.g. `"the
plain \`blocking\` gate (no -human suffix) must not match — Phase 26 near-miss distinction"`). A new
`phase_has_blocking_human_checkpoint_reads_execution_root_in_worktree_mode`-style test should follow
this exact shape: `write_phase_file` into a worktree-standing-in directory, call the function with
both roots distinguished, assert the checkpoint is still found.

---

## Shared Patterns

### Descriptively-named small tests, never table-driven macros

**Source:** `pipeline_outcomes.rs:1009-1130`, `agent_result.rs:5268-5543`, `verify.rs:242-314` — all
read verbatim this session.
**Apply to:** every new test this phase adds (the 42-cell sweep, the D-15 fourth case, the two
999.76 fixtures).
Confirmed pattern across all three files: one `#[test] fn descriptive_sentence_name()` per case,
preceded by a doc comment naming the decision (`D-NN`) it pins and what specifically it asserts.
RESEARCH.md's own Open Question #2 recommends this explicitly: *"follow the existing convention
(many small, descriptively-named `#[test]` functions) rather than introducing a table-driven macro
pattern this codebase does not currently use elsewhere in these two files."* No table-driven macro,
`rstest`, or parameterized-test crate exists anywhere in the files read this session — do not
introduce one for the 42-cell sweep; write 42 (or a compressed but still individually-named/grouped)
set of assertions in the same struct-literal style shown above, with the required named controls
(positive control + two `Ambiguous` cells + their `layer0=false` mirrors) each getting their own
named `#[test]`.

### `AgentResult` struct literals in tests always set `decided_by_layer` explicitly

**Source:** `pipeline_outcomes.rs:1020-1028`, `:1072-1080`, `:1116-1124`; `agent_result.rs:38-42`'s
doc comment.
**Apply to:** any new test constructing an `AgentResult` value, especially the 42-cell sweep.
Never omit the field and rely on its default — every existing example sets it explicitly
(`Some(0)`), and the field's own doc comment states `None` is reserved for fixtures that
deliberately don't route through the real cascade. Omitting it silently produces `layer0 = false`
after D-06's normalisation, which is exactly the trap D-08's amendment warns collapses half the
sweep to untested.

### Wildcard-free exhaustive match, `_` allowed only in non-status/non-outcome positions

**Source:** `crates/devflow-core/src/outcome_policy.rs:38-66` (`decide_action`), cited explicitly by
CONTEXT.md as the precedent D-06 continues.
**Apply to:** `classify_validate_outcome`'s rewrite in `pipeline_outcomes.rs`.
Every `AgentStatus` variant must appear by name in the match (never behind `_`), even when two
variants share a destination — in that case, add an inline comment explaining the shared destination
is intentional, exactly as `decide_action` does for `Failed`/`Unknown` and separately for
`IdleTimeout`. Positions that are genuinely don't-care (the `layer0` boolean, the `verdict` in some
arms) may use `_`; the `status`/`AgentStatus` position specifically must not.

### Evidence directory layout for per-stage captures (999.73 criteria 1/2)

**Source:** `.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/{30a,30c,30c-evidence-scrubbed,30c-evidence-operator}-evidence/` — all four directories listed via `ls -la` this
session; `run.log` contents for `30c-evidence/` and `30c-evidence-scrubbed/` read verbatim this
session and diffed.
**Apply to:** the new `.planning/phases/34-.../{unit}-evidence/` directories this phase's capture
campaign produces.

Concrete file inventory, this session's `ls -la`:

| Directory | Files | Role |
|---|---|---|
| `30a-evidence/` | `README.md`, `raw_output.jsonl`, `raw_output_v2.jsonl`, `raw_output_v3.jsonl`, `run_experiment.py`, `run_experiment_v2.py`, `run_experiment_v3.py` | Exploratory harness runs; script alongside its own output, multiple numbered iterations kept side by side rather than overwritten |
| `30c-evidence/` | `raw_output.jsonl` (64939 bytes), `run.log`, `stderr.log` (0 bytes) | Baseline trial — no `run_experiment*.py` because 30c's harness lived outside this directory |
| `30c-evidence-scrubbed/` | `raw_output.jsonl` (55898 bytes), `run.log`, `stderr.log` (0 bytes) | A SEPARATE run with agent-session env markers programmatically scrubbed before launch — different byte count from the baseline, confirming it is an independent run, not a redacted copy |
| `30c-evidence-operator/` | `raw_output.jsonl` (56165 bytes), `run.log`, `stderr.log` (192 bytes, non-empty — this run produced stderr output the other two did not) | A THIRD, genuinely-plain-shell operator trial, outside any agent session |

`run.log` format, verbatim, both variants (`30c-evidence/run.log` vs. `30c-evidence-scrubbed/run.log`,
diffed this session — only the header comment and one `## Environment scrub` line differ):
```
# 30c run log — monitor-environment replication harness
claude_cli_version: 2.1.220
expected_cli_version: 2.1.220
workdir: ~/Github/devflow
staged_output_dir: /tmp/devflow-30c-19cxeek9
staged_dir_outside_repo: True
argv: claude -p --input-format stream-json --output-format stream-json --verbose --dangerously-skip-permissions

## Replicated from spawn_monitor
launch_via_sh_c: true
detached_start_new_session: true
stdin_tty: false / stdout_tty: false / stderr_tty: false
stderr_separate_file: true

## Deliberate deviation (the variable under test)
harness_holds_child_stdin_open: true

## Environment scrub (names only)
scrub_list_source: crates/devflow-core/src/git.rs (0 of the parsed names were present and removed)
removed_variables: (none were set)
```
(the scrubbed variant's `## Environment scrub` block instead reads `trial: 2 — agent-session
markers ALSO scrubbed` followed by the same `scrub_list_source` line — the only content divergence
between the two `run.log` files this session's diff found).

**What the raw/scrubbed/operator split concretely means, for this phase's per-stage directories:**
`raw_output.jsonl` is always the capture copied straight out of `.devflow/`, unmodified. "Scrubbed"
is a SEPARATE run (not a post-hoc redaction of the raw file) with agent-session environment markers
(`CLAUDE_*`/`AI_AGENT*`/`ANTHROPIC*`) removed before launch — RESEARCH.md's redaction-field table
additionally names `home_path`, `os_username`, `session_identifier` as the three fields replaced
with placeholders (`<cwd>`, `<session-01>`) before a capture is committed. "Operator" is a third,
independent variant run entirely outside any agent session. For Phase 34's per-stage captures
(single n=1 run per stage, per D-10), the minimum reproduction is `raw_output.jsonl` (copied from
`.devflow/`) + `run.log` (a short human-readable summary: command invoked, stage, agent version,
outcome) with the three PII fields placeholder-scrubbed before commit — a `README.md` is optional
but cheap, per `30a-evidence/`'s example, and useful given four stages each need their own
provenance note (command, build, git commit of `STREAM_JSON_STAGES`).

## No Analog Found

None — every file this phase touches is an existing source file with an established in-file test
convention; there is no genuinely new file (controller/component/service skeleton) requiring a
cross-codebase analog search. The evidence-directory layout is the one artifact that is "new" in the
sense of not yet existing for Phase 34, but its analog (Phase 30's four evidence directories) is a
complete, concrete precedent, not a gap.

## Metadata

**Analog search scope:** `crates/devflow-cli/src/pipeline_launch.rs`,
`crates/devflow-cli/src/pipeline_outcomes.rs`, `crates/devflow-core/src/agent_result.rs`,
`crates/devflow-core/src/outcome_policy.rs`, `crates/devflow-core/src/verify.rs`,
`.planning/milestones/v2.3.0-phases/30-keep-the-session-alive-past-turn-end/` (evidence directories
and `run.log` files).
**Files scanned:** 6 source/doc files read directly this session (all with `file:line` citations
above), plus 4 evidence directories inventoried via `ls -la`.
**Pattern extraction date:** 2026-08-05
