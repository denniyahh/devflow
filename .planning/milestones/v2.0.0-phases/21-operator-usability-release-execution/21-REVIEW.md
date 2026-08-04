---
phase: 21-operator-usability-release-execution
reviewed: 2026-07-23T21:34:44Z
depth: standard
files_reviewed: 5
files_reviewed_list:
  - crates/devflow-cli/src/commands.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/src/parallel.rs
  - crates/devflow-cli/src/staleness.rs
  - crates/devflow-core/src/agent_result.rs
findings:
  critical: 0
  warning: 3
  info: 1
  total: 4
status: issues_found
---

# Phase 21: Code Review Report

**Reviewed:** 2026-07-23T21:34:44Z
**Depth:** standard
**Files Reviewed:** 5
**Status:** issues_found

## Summary

Reviewed the Phase 21 diff (against `c42d84dc304d2055d3d23205d60040c0ed20f598`) across all five listed files: `devflow status`/`gate show` discoverability additions (21a), `doctor`'s planning-doc release-drift check (21b), content-aware build staleness (21d), and `sequentagent` second-agent slot tracking (21c). `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` both run clean on the current tree, and the phase's own test suite (visible in the diff) is unusually thorough — most of the obvious edge cases I went looking for (Indeterminate never blocking, docs-only ranges never blocking, mixed docs+source ranges still blocking, git-failure-fails-toward-Stale, slot-guard clears on every exit path) are already covered by a dedicated regression test.

I did not find a crash, data-loss, or security-relevant defect in the new code. I did find one genuine code-duplication risk that undercuts its own doc-comment's guarantee, one hardcoded literal that bypasses an existing named constant for the same value, and a reintroduction of a full-file-rescan pattern the codebase's own prior fix (14-CR-10) was written to eliminate, right inside the same function that fix lives in. All three are quality/maintainability findings (Warning), not correctness bugs — the current runtime behavior is correct in every case I traced.

## Warnings

### WR-01: `gate_show`'s stage-auto-resolution logic is copy-pasted from `gate_respond`, not shared — contradicts its own doc comment's "can never drift" claim

**File:** `crates/devflow-cli/src/commands.rs:814-844` (new `gate_show`), duplicating `crates/devflow-cli/src/commands.rs:748-773` (pre-existing `gate_respond`)

**Issue:** `gate_show`'s doc comment states it "Mirrors `gate_respond`'s stage auto-resolve-single-open-gate logic ... so the two commands' gate-resolution behavior can never drift." In fact the `[] => Err(...)` / `[one] => stage` / `many => Err(...)` block, including both error message strings verbatim, is copy-pasted rather than factored into a shared function. The claim in the comment is currently true only by coincidence — a future edit to `gate_respond`'s resolution logic (e.g. changing the ambiguous-gate error text, or adding a new disambiguation rule) has no compiler-enforced or test-enforced link to `gate_show`, so the two commands *can* drift despite what the comment promises. This is exactly the kind of assertion a reviewer should not take on faith.

**Fix:** Extract the shared resolution into one function both callers use:
```rust
/// Resolve which stage to act on when `--stage` was omitted: the phase's
/// single open gate, or an error naming `devflow gate list` (none open) /
/// asking for `--stage` (several open). Shared by `gate_respond` and
/// `gate_show` so the two commands' resolution behavior cannot drift.
fn resolve_single_open_gate_stage(project_root: &Path, phase: u32) -> Result<Stage, CliError> {
    let open: Vec<_> = Gates::list_open(project_root)
        .into_iter()
        .filter(|g| g.phase == phase)
        .collect();
    match open.as_slice() {
        [] => Err(CliError::Message(format!(
            "no open gate for phase {phase} — see `devflow gate list`"
        ))),
        [one] => Ok(one.stage),
        many => Err(CliError::Message(format!(
            "phase {phase} has several open gates ({}) — pass --stage",
            many.iter().map(|g| g.stage.to_string()).collect::<Vec<_>>().join(", ")
        ))),
    }
}
```
and have both `gate_respond` and `gate_show` call `stage.map_or_else(|| resolve_single_open_gate_stage(project_root, phase), Ok)?`.

### WR-02: planning-doc staleness check hardcodes the literal `"main"` instead of reusing the existing `devflow_core::config::MAIN` constant

**File:** `crates/devflow-cli/src/commands.rs:2285` (`collect_planning_doc_findings`)

**Issue:** `let mut lookup = |tag: &str| tag_exists_and_reachable(project_root, tag, "main");` hardcodes the branch name as a bare string literal. `devflow_core::config` already defines `pub const MAIN: &str = "main";` (and a `GitFlowConfig.main` field) as the single named source of truth for this exact value, and `config.rs`'s own module doc explicitly flags the git-flow branch model as something Phase 16's D-03 "deliberately reopened ... for a minimal `devflow.toml`" — i.e. this is documented as a value that may become configurable later. If/when it does, this call site silently keeps using the wrong branch name while every other consumer of `GitFlowConfig`/`MAIN` picks up the change, producing false "Problem"-severity findings in `doctor --json` (which every other reconciliation check in this file goes out of its way to avoid — see the `PLANNING_DOC_STALENESS_CUTOFF` comment's explicit "alert-fatigue" concern). Today the literal happens to match the constant, so there's no live bug, but it's a second, unlinked source of truth for a value the codebase already names.

**Fix:**
```rust
use devflow_core::config::MAIN;
// ...
let mut lookup = |tag: &str| tag_exists_and_reachable(project_root, tag, MAIN);
```

### WR-03: `gate_show` performs `Gates::list_open` twice, creating a narrow TOCTOU window

**File:** `crates/devflow-cli/src/commands.rs:814-852`

**Issue:** `gate_show` calls `Gates::list_open(project_root)` once to resolve `stage` (when `None`), then calls it again immediately after to `.find()` the matching `OpenGate`. Between the two calls another operator/process could answer the gate (`gate approve`/`gate reject`), causing the second lookup to miss it and `gate_show` to fail with "no open gate for phase {phase} stage {stage}" even though the gate was open moments earlier and the auto-resolution had just proven it. Low real-world likelihood (single-operator CLI use), but avoidable.

**Fix:** Fetch `Gates::list_open(project_root)` once and reuse the same `Vec<OpenGate>` for both the stage-resolution filter and the final `.find()`.

## Info

### IN-01: `latest_stage_launched_ts` reintroduces the per-phase full-file rescan that 14-CR-10 (two lines above it, in the same function) was written to eliminate

**File:** `crates/devflow-cli/src/commands.rs:553-564` (`latest_stage_launched_ts`), called per-phase from `status()` at `crates/devflow-cli/src/commands.rs:650-654`

**Issue:** `status()` already computes `last_events = events::last_events_by_phase(project_root)` once, up front, specifically because (per its own adjacent comment) "14-CR-10: one pass over events.jsonl for every phase's last event, instead of a full-file scan per phase." The new `latest_stage_launched_ts(project_root, state.phase)` call sits inside the same per-phase loop and does exactly the full-file `read_to_string` + parse + filter scan that comment describes as the thing being avoided — once per active phase. Functionally correct (and out of scope per this review's performance exclusion), but it's a quality regression against a documented, adjacent anti-pattern in the very function that fixed it, and would be easy to fold into the existing single-pass `last_events_by_phase`-style helper (e.g. by tracking the last `stage_launched` event's `ts` per phase in that same pass) rather than re-scanning per phase.

**Fix:** Not required for this review (performance is out of v1 scope), but worth a follow-up: extend `events::last_events_by_phase` (or add a sibling single-pass helper) to also return each phase's last `stage_launched` timestamp, and have `status()` read from that instead of calling `latest_stage_launched_ts` per phase.

---

_Reviewed: 2026-07-23T21:34:44Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
