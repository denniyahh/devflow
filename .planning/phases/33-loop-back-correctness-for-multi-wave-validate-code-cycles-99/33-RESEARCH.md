# Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles - Research

**Researched:** 2026-08-04
**Domain:** Internal Rust pipeline state machine correctness (DevFlow's own `devflow-core`/`devflow-cli` crates) — no third-party library or external API is involved. This is a defect-fix phase against known, already-diagnosed code paths, not a new-stack adoption.
**Confidence:** HIGH — every claim below is `[VERIFIED]` against source read directly in this session (file + line range), not training-data recall. The one genuinely open design question (D-03) is answered with an explicit recommendation and full tradeoff analysis, not left to the planner.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**999.65 — mid-arc vs. genuine-gaps signal**
- **D-01:** The loop-back distinguishes "Validate correctly found the phase mid-arc/unbuilt" from "Validate found real defects in already-built work" using **file existence of `{N}-VERIFICATION.md` alone** — no `{N}-VERIFICATION.md` → issue plain `/gsd-execute-phase {N}`; file exists → issue `--gaps-only`, unchanged. This matches the phase's own success criteria literally and needs no parsing of VERIFICATION.md's internal finding-type breakdown. — **Reversibility:** reversible — a pure decision-function change, no persisted-state shape involved.
- **D-02:** The fix touches only the **3 call sites inside `handle_validate_outcome`** (`pipeline_outcomes.rs:255`, `:285`, `:292` — the Ambiguous-gate loop-back, the consecutive-failure gate loop-back, and the plain Failed loop-back). The 4th `FixType::GapsOnly` call site, inside `handle_ship_outcome` (`:321`, a human rejecting a completed Ship and looping back to Code), is explicitly **out of scope** — by the time Ship runs the phase is by definition not mid-arc, so `--gaps-only` is already correct there. — **Reversibility:** reversible.

**999.66 — forward-progress signal for the consecutive_failures reset**
- **D-03:** The signal that distinguishes "genuine forward progress" (reset the counter) from "same problem again" (keep counting toward `MAX_CONSECUTIVE_FAILURES`) is **deferred to the phase researcher**. Two candidates were surfaced and neither was picked outright: (a) track the Code-stage commit set/HEAD at the moment Validate fails, reset only if Code has produced new commits since then; (b) compare Validate's reported findings/reason text across loop-backs, reset only if the findings differ from the immediately preceding failure. **Do not** default to "reset on every loop-back" — that reintroduces 18d's original unreachable-ceiling bug in a different form (hard constraint, not a preference). — **Reversibility:** one-way for whichever heuristic ships — needs a new persisted `State` field, so the reset predicate must have a safe default for its absence in older persisted state.

### Claude's Discretion
- Exact shape of any new `State` field(s) needed to support D-03's eventual heuristic (naming, type, serde defaulting for older persisted state).
- Whether `handle_validate_outcome`'s `FixType` enum needs a new variant (e.g., distinct from `GapsOnly`) versus keeping `FixType::GapsOnly`/`AuditFix` as-is and doing the mid-arc/gaps branch upstream of the `loop_back_to_code` call.

### Deferred Ideas (OUT OF SCOPE)
- 999.73 (widen stream-json launch path beyond `Stage::Code`) and 999.74 (`classify_validate_outcome` trusting self-reported `verdict`) — already split into Phase 34 by an explicit ROADMAP.md decision predating this discussion; not re-litigated here.
- Whether `handle_ship_outcome`'s Ship→Code loop-back (`pipeline_outcomes.rs:321`) has its own latent defect — out of scope for this phase (D-02), flagged in case a future dogfood run surfaces a real problem there.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DOGFOOD-01 | Operator can run a 3+ wave unattended phase where Validate correctly reports the phase mid-arc/incomplete, without the loop-back issuing an unresolvable `--gaps-only` command (999.65) | `## Architecture Patterns` Pattern 1 (D-01 mechanism: reuse the `phase_review_path` idiom at `agent_result.rs:2525-2541` for a new `{N}-VERIFICATION.md`-existence check); `## Code Examples` |
| DOGFOOD-02 | Operator can run a 3+ wave unattended phase in `auto` mode without a false "3 consecutive failures" gate firing on healthy wave-by-wave progress (999.66) | `## Architecture Patterns` Pattern 2 and `## D-03 Recommendation` (the full candidate analysis and recommended commit-count signal, grounded in `agent_result.rs:1850-1934`'s existing git-derived commit-counting precedent) |
</phase_requirements>

## Summary

Both defects live entirely inside three files already named in CONTEXT.md's canonical refs, and both are read-and-confirmed against live source in this session: `crates/devflow-cli/src/pipeline_outcomes.rs` (`handle_validate_outcome`, the three in-scope `FixType::GapsOnly` call sites, and where `consecutive_failures` is incremented), `crates/devflow-cli/src/pipeline_gate.rs` (`transition`, `prepare_loop_back_to_code`), and `crates/devflow-core/src/mode.rs` (`transition_resets_consecutive_failures`, `MAX_CONSECUTIVE_FAILURES`, `Mode::should_gate`). All cited line numbers in CONTEXT.md's canonical refs matched the live file exactly on re-read — no drift to correct.

999.65's fix is mechanical once D-01/D-02 are read literally: add a `{N}-VERIFICATION.md`-existence check upstream of the three in-scope `loop_back_to_code(..., FixType::GapsOnly)` calls, and route to a plain-command fix when the file is absent. No existing production function performs this check today (confirmed by grep — only doc-comments mention `VERIFICATION.md`), but the codebase has an exact structural precedent to mirror: `phase_review_path` in `agent_result.rs:2525-2541`, which scans `.planning/phases/` for a `{phase:02}-`-prefixed directory and checks for a named artifact inside it. A new `phase_verification_exists(project_root, phase) -> bool` following that identical shape is the natural implementation.

999.66 is the harder problem and is where CONTEXT.md explicitly deferred a decision to this research pass. After reading `transition()`, `prepare_loop_back_to_code`, `transition_resets_consecutive_failures`, and `handle_validate_outcome`'s counter-increment site directly, the mechanism is clear: `prepare_loop_back_to_code` (`pipeline_gate.rs:130-158`) never resets `consecutive_failures` at all (it doesn't touch that field), and the counter is only ever incremented — never conditionally reset — inside `handle_validate_outcome` (`pipeline_outcomes.rs:264-270`) on every `ValidateResult::Failed`. This is by design for the single-wave case (18d's fix, so `MAX_CONSECUTIVE_FAILURES` is reachable at all) but wrong for the multi-wave case, where several of those "failures" are really just Validate correctly reporting "more waves remain," not the same defect recurring.

**Primary recommendation:** implement D-03 using the commit-count signal (CONTEXT.md's candidate (a), which the ROADMAP.md backlog entry itself already leans toward — see `## D-03 Recommendation` for the full analysis and why the findings-text candidate (b) is materially weaker for this exact codebase). Reuse the git-derived (not agent-self-reported) commit-counting mechanism `evaluate_layer2` already established at `agent_result.rs:1862-1881` (`git rev-list --count {develop}..{branch}` against the phase's feature branch, computed from `project_root` — correctly branch-aware regardless of whether the agent ran inside a worktree, since git refs are shared across a repo's worktrees). Store the commit count observed at the moment of the most recent Validate failure in a new `State` field; at the next Validate failure, compare current count to stored count — a higher count means Code did new work since the last failure (reset), an unchanged count means nothing changed (keep accumulating, preserving criterion 4's safety gate).

## Architectural Responsibility Map

This project is a single-process Rust CLI tool driving an external agent subprocess through a persisted state machine — not a multi-tier web application, so the standard browser/frontend/API/CDN/DB table does not apply. The equivalent tiers for this phase:

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Loop-back command selection (999.65) | `devflow-cli::pipeline_outcomes` (orchestration/I/O layer) | `devflow-core::prompt` (pure prompt-string builder) | The decision ("which command to issue") needs I/O (file existence check under `.planning/phases/`), which only the CLI crate performs; `prompt::fix_prompt` stays a pure string-formatter, unaware of the decision that chose its `FixType` input. |
| Consecutive-failure reset predicate (999.66) | `devflow-core::mode` (pure decision function) | `devflow-cli::pipeline_outcomes` (I/O: computing the commit-count input) | Established codebase pattern (`transition_resets_consecutive_failures`): the actual decide-to-reset-or-not logic is a small pure function taking already-computed inputs; the CLI layer is responsible for doing the I/O (`git rev-list --count`) and passing the result in — this phase should extend that pattern, not special-case logic inline inside `prepare_loop_back_to_code` (an anti-pattern CONTEXT.md explicitly names). |
| Persisted state shape (new field for D-03) | `devflow-core::state::State` | — | `State` is the sole persistence boundary (`.devflow/state.json`); every prior counter-style field (`consecutive_failures`, `infra_failures`, `preflight_retries`, `checkpoint_resumes`) lives here with `#[serde(default)]`, and the new field must follow the same convention. |
| Commit-count derivation (999.66's input signal) | `devflow-core::agent_result` (existing git-shelling logic) or a new sibling helper in the same module | `devflow-core::git` (the `GitFlow` wrapper, has the `rev-parse`/branch idioms but not this exact query) | `evaluate_layer2` (`agent_result.rs:1850-1934`) already computes exactly this number for a different purpose (the "no work done" Layer-2 fallback) — extracting or duplicating that ~15-line block is lower-risk than inventing a new git-query mechanism. |

## Standard Stack

Not applicable in the conventional sense — this phase adds no new external dependency. `serde`/`serde_json` (already a workspace dependency, used throughout `state.rs`) is the only crate touched, and only through the existing `#[serde(default)]` pattern already used by every prior counter field. No `npm view`/`pip index versions`/`cargo search` verification is needed because nothing new is being added to `Cargo.toml`.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Reusing `git rev-list --count` via a `std::process::Command` shell-out (existing pattern) | A git library crate (e.g. `git2`) for the commit-count query | Rejected implicitly by precedent: the entire codebase (`git.rs`, `agent_result.rs`, `hooks.rs`) already shells out to the system `git` binary rather than linking a git library, and 27-04's "hermetic command" invocation pattern (see ROADMAP 999.61) exists specifically to harden that shelling-out approach, not replace it. Introducing `git2` here would be a large, unrelated architectural change for a one-off count query — clearly out of scope. |

## Package Legitimacy Audit

Not applicable — this phase installs no external packages. No `Cargo.toml` changes are anticipated; the fix is pure logic plus one new `#[serde(default)]` field on an existing struct.

## Architecture Patterns

### System Architecture Diagram

```
                     ┌─────────────────────────────────────────────┐
                     │  pipeline_launch::advance()                  │
                     │  (evaluates AgentResult for the current      │
                     │   stage, classifies it, dispatches)          │
                     └───────────────────┬───────────────────────────┘
                                          │ Stage::Validate result
                                          ▼
                     ┌─────────────────────────────────────────────┐
                     │  pipeline_outcomes::classify_validate_outcome│
                     │  (pure: AgentResult -> ValidateOutcome)      │
                     └───────────────────┬───────────────────────────┘
                                          │ Passed / Failed / Ambiguous
                                          ▼
              ┌───────────────────────────────────────────────────────────┐
              │  pipeline_outcomes::handle_validate_outcome                │
              │                                                             │
              │  [NEW — 999.65] does {N}-VERIFICATION.md exist?             │
              │     no  -> FixType::<plain continue>                        │
              │     yes -> FixType::GapsOnly  (unchanged)                   │
              │                                                             │
              │  [EXISTING — increments consecutive_failures on Failed]     │
              │  [NEW — 999.66] did Code's commit count on the phase        │
              │     branch increase since the LAST recorded failure?        │
              │     yes -> reset toward 1 instead of accumulating           │
              │     no  -> accumulate as today (same-problem safety gate)   │
              │                                                             │
              │  should_gate(Validate, consecutive_failures)?                │
              │     yes -> run_gate (human)                                 │
              │     no  -> loop_back_to_code(fix)                           │
              └───────────────────────────┬─────────────────────────────────┘
                                           │
                                           ▼
              ┌───────────────────────────────────────────────────────────┐
              │  pipeline_gate::prepare_loop_back_to_code                  │
              │  sets state.stage = Code DIRECTLY (never calls             │
              │  transition() — this is why the reset predicate at         │
              │  mode::transition_resets_consecutive_failures is never     │
              │  consulted on THIS hop; the counter's reset/accumulate     │
              │  decision must therefore be made upstream, in              │
              │  handle_validate_outcome, before this function runs)       │
              └───────────────────────────┬─────────────────────────────────┘
                                           │ launch_stage(prompt) -> Code agent
                                           ▼
                                  new commits land on
                                  feature/phase-{NN} (or none, if stuck)
                                           │
                                           ▼
                     Code -> Validate again via transition() (pipeline_gate.rs:51)
                     — consecutive_failures NOT reset here either
                     (transition_resets_consecutive_failures returns false
                     for exactly the (Code, Validate) hop, mode.rs:111-113)
                                           │
                                           └──────────► back to top
```

### Recommended Project Structure

No new files or directories — every change lands inside the existing three modules named in CONTEXT.md's canonical refs, plus (optionally) a small addition to `devflow-core::state` for the new field:

```
crates/devflow-core/src/
├── mode.rs          # extend with a new pure predicate parallel to
│                    # transition_resets_consecutive_failures, taking the
│                    # commit-count comparison result as input (999.66)
├── state.rs          # new #[serde(default)] field recording the commit
│                    # count observed at the last Validate failure (999.66)
├── prompt.rs          # FixType — Claude's Discretion: extend with a new
│                    # variant, or keep 2 variants and branch upstream
│                    # (999.65)
└── agent_result.rs   # candidate home for a small commit-count helper,
                     # sibling to evaluate_layer2's existing logic (999.66)

crates/devflow-cli/src/
├── pipeline_outcomes.rs  # handle_validate_outcome — both fixes' call site
│                         # (999.65's VERIFICATION.md check AND 999.66's
│                         # commit-count comparison both belong here, next
│                         # to the existing consecutive_failures increment
│                         # at :264-270)
└── pipeline_gate.rs      # prepare_loop_back_to_code — NOT where the
                          # counter logic goes (per CONTEXT.md's Established
                          # Patterns note); may still need touching for
                          # 999.65 if a new FixType variant needs a
                          # matching prompt::fix_prompt arm
```

### Pattern 1: 999.65's mid-arc/genuine-gaps signal (D-01/D-02, confirmed against live source)

**What:** `handle_validate_outcome` currently calls `loop_back_to_code(project_root, state, FixType::GapsOnly)` unconditionally at its three in-scope call sites. The fix inserts a file-existence check immediately before each call.

**Confirmed call sites** `[VERIFIED: crates/devflow-cli/src/pipeline_outcomes.rs:254-257,283-286,290-293]`:
```rust
// :246-258 (Ambiguous-gate loop-back arm)
let result = match outcome {
    ValidateOutcome::Ambiguous(detail) => {
        let context = format!(
            "[never-silent] validate ambiguous: {}",
            truncate_reason(&detail)
        );
        return match run_gate(project_root, state, Stage::Validate, &context)? {
            GateAction::Advance => transition(project_root, state, Stage::Ship),
            GateAction::LoopBack(_) => {
                loop_back_to_code(project_root, state, FixType::GapsOnly)
            }
            GateAction::Abort(reason) => abort(project_root, state, &reason),
        };
    }
    ...

// :283-287 (consecutive-failure gate loop-back arm)
return match run_gate(project_root, state, Stage::Validate, &context)? {
    GateAction::Advance => transition(project_root, state, Stage::Ship),
    GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
    GateAction::Abort(reason) => abort(project_root, state, &reason),
};

// :290-293 (plain Failed tail match, the common auto-loop path)
match result {
    ValidateResult::Passed => transition(project_root, state, Stage::Ship),
    ValidateResult::Failed => loop_back_to_code(project_root, state, FixType::GapsOnly),
}
```

**The 4th, out-of-scope call site** `[VERIFIED: crates/devflow-cli/src/pipeline_outcomes.rs:319-321]` — inside `handle_ship_outcome`, confirmed identical to CONTEXT.md's citation:
```rust
match run_gate_with_timeout(...)? {
    GateAction::Advance => finish_workflow(project_root, state),
    GateAction::LoopBack(_) => loop_back_to_code(project_root, state, FixType::GapsOnly),
    GateAction::Abort(reason) => abort(project_root, state, &reason),
}
```
D-02's "by the time Ship runs the phase is by definition not mid-arc" reasoning holds structurally: `Stage::Ship` is only reached via `handle_validate_outcome`'s `transition(project_root, state, Stage::Ship)` calls, which only fire on `ValidateResult::Passed` or a human's `GateAction::Advance` — both require Validate to have already judged the phase complete. Leave this call site untouched.

**Mechanism to reuse (no existing function does this check today — confirmed by grep, zero production hits for `VERIFICATION.md` outside doc comments):** `[VERIFIED: crates/devflow-core/src/agent_result.rs:2525-2541]`
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
A new `phase_verification_exists(project_root: &Path, phase: u32) -> bool` (or `Option<PathBuf>` to mirror the exact shape) that substitutes `{phase:02}-VERIFICATION.md` for `{phase:02}-REVIEW.md` is the direct, minimal-risk implementation — same directory-prefix scan idiom the codebase already trusts for an adjacent artifact-existence question.

**When to use:** Call this check once per in-scope `loop_back_to_code` site, immediately before selecting `FixType`. Per D-02's discretion note, the cleanest shape is likely a single small helper inside `handle_validate_outcome` (e.g. `fn select_loop_back_fix(project_root, phase) -> FixType`) called identically at all three sites, rather than duplicating the check three times.

### Pattern 2: 999.66's reset predicate — the existing pure-function idiom to extend

**What:** `[VERIFIED: crates/devflow-core/src/mode.rs:87-113]` — the existing predicate, and its doc comment explaining exactly why the Code→Validate hop is excluded:
```rust
/// Whether `transition()` should zero
/// [`crate::state::State::consecutive_failures`] when moving from `from` to
/// `to`.
///
/// `consecutive_failures` is meant to count repeated Code↔Validate CYCLES —
/// each cycle is a full loop through Code, then Validate, then (on failure)
/// back to Code again. But the Code→Validate hop is crossed on *every
/// single cycle*, including the ones that are about to fail. Resetting the
/// counter on that specific hop means it can never accumulate past 1, so
/// [`MAX_CONSECUTIVE_FAILURES`] — the ceiling that exists specifically to
/// bound this loop — is unreachable (18d). Every other transition is
/// genuine forward progress out of the Code↔Validate loop (or the initial
/// Define→Plan→Code entry into it) and correctly clears the counter.
...
pub fn transition_resets_consecutive_failures(from: Stage, to: Stage) -> bool {
    !matches!((from, to), (Stage::Code, Stage::Validate))
}
```
This function is called only from `transition()` (`pipeline_gate.rs:95-97`) — never from `prepare_loop_back_to_code`, which is exactly the bypass CONTEXT.md's domain section describes. Critically, `prepare_loop_back_to_code` **does not need to call this predicate** to fix 999.66: it never touches `consecutive_failures` at all today (confirmed — the function only mutates `stage`, `gate_pending`, and cleans up gate files), and it must not start doing so, because the counter's increment already happens earlier, inside `handle_validate_outcome`, before `loop_back_to_code` is ever invoked. The fix belongs entirely upstream of `prepare_loop_back_to_code`, at the increment site itself.

**Confirmed increment site** `[VERIFIED: crates/devflow-cli/src/pipeline_outcomes.rs:264-270]`:
```rust
if result == ValidateResult::Failed {
    // Now that the counter genuinely accumulates (18d), an unbounded
    // loop could otherwise overflow it and wrap to 0, silently
    // restoring the unreachable-ceiling bug in a slower form.
    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    workflow::save_state(state)?;
}
```
This is the single site to change for 999.66: instead of an unconditional `saturating_add(1)`, branch on whether Code produced new commits since the last recorded failure (see `## D-03 Recommendation` below for the exact signal and its threading).

**When to use:** Any future gate-affecting decision that needs more than `(from, to)` stage names as input should follow this same shape — a small, named, unit-testable pure function that the I/O-performing caller consults, not inline conditional logic buried in the caller. CONTEXT.md's own `<code_context>` section states this explicitly as the pattern to preserve.

### Anti-Patterns to Avoid
- **Resetting `consecutive_failures` inside `prepare_loop_back_to_code`:** this function fires on EVERY loop-back, healthy or not — making the reset unconditional here is structurally identical to "reset on every loop-back," the explicitly forbidden naive fix. The reset decision must be conditional on the forward-progress signal and must live where the increment already lives (`handle_validate_outcome`), not where the stage assignment lives (`prepare_loop_back_to_code`).
- **Widening `transition_resets_consecutive_failures`'s signature to accept I/O-derived data:** this would break its "pure function of `(from, to)`" contract and its direct unit-testability (`mode.rs`'s existing tests call it with bare `Stage` values, no `Path`/`git` setup). Keep the git-derived comparison as I/O performed by the caller (`handle_validate_outcome`), and pass only the resulting boolean/comparison into any new pure predicate.
- **Trusting the Validate agent's self-reported findings text as the sole forward-progress signal (candidate (b)):** see `## D-03 Recommendation` for the full argument — this is the same class of trust-boundary weakness Phase 34's DOGFOOD-04 (999.74) exists to close on the verdict field; introducing a second self-report-trusting mechanism in Phase 33 would work against that separately-planned hardening.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Counting commits on the phase's feature branch | A new git-shelling helper from scratch | Extract or closely mirror `evaluate_layer2`'s existing `git rev-list --count {develop}..{branch}` block (`agent_result.rs:1862-1881`) | Already proven, already tested, already handles the "branch doesn't exist yet" edge case (`branch_exists` guard) — duplicating the logic with a slightly different guard is a likely source of a subtle divergence bug; extracting a small shared helper (or calling the existing function's underlying block directly) avoids that. |
| Locating a phase's `.planning/phases/{NN}-*/` directory by numeric prefix | A new glob/directory-scan routine | Mirror `phase_review_path` (`agent_result.rs:2525-2541`) exactly, substituting the target filename | Same scan idiom already trusted for an adjacent artifact (`REVIEW.md`); no reason to invent a second directory-resolution strategy in the same crate. |

**Key insight:** both of this phase's fixes have a directly analogous, already-shipped precedent living in the same crate (`agent_result.rs`) that solves a structurally identical sub-problem for a different purpose. The lowest-risk implementation path for both 999.65 and 999.66 is "do what this codebase already does elsewhere," not new design.

## D-03 Recommendation

CONTEXT.md requires this section to lay out both surfaced candidates (and any other viable option) against the real `transition()`/`prepare_loop_back_to_code` code paths, name concrete tradeoffs, and recommend one. This section is that analysis.

### Candidate A — commit-count/HEAD signal (CONTEXT.md's candidate (a); ROADMAP.md's own stated lean)

**Mechanism.** At the point `handle_validate_outcome` would increment `consecutive_failures` (`pipeline_outcomes.rs:264-270`), first compute the current commit count on the phase's feature branch using the exact query `evaluate_layer2` already performs `[VERIFIED: crates/devflow-core/src/agent_result.rs:1862-1881]`:
```rust
let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase);
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
Compare this count against a new `State` field recording the count observed at the *previous* Validate failure. If the count increased, Code did new work since that failure — reset the counter (to 1, for this new failure) and update the stored baseline to the current count. If the count is unchanged, nothing new landed — accumulate as today.

**Why it works across a worktree.** `git_command(project_root)` runs from `project_root`, not `state.worktree_path` — and this is correct, not a gap: git worktrees share the same object database and refs, so a commit made inside a linked worktree checkout is immediately visible to `git rev-list --count` run from the main checkout. `evaluate_layer2` already relies on exactly this property (it is called with `project_root`, never the worktree path, at every one of its call sites), so no new worktree-awareness is needed.

**Tradeoffs:**
- *Correctness:* Objective and tamper-resistant — the signal comes from git's own object graph, not from anything the agent self-reports. An agent cannot claim "I made progress" without actually committing something; this closes off a whole class of self-report gaming.
- *Known weakness (must be documented, not hidden):* an agent that makes a trivial, non-substantive commit each cycle (e.g., a comment-only edit) without actually addressing Validate's finding would incorrectly reset the counter, weakening criterion 4's safety gate in that specific adversarial-or-buggy-agent scenario. This is a real but low-probability failure mode in practice: GSD's `/gsd-execute-phase {N}` and `/gsd-execute-phase {N} --gaps-only` both drive plan-based execution that normally produces substantive commits per plan/wave, not empty busywork commits — and this same class of risk already exists, unaddressed, in `evaluate_layer2`'s own "no work done" gate (a single trivial commit already defeats that check today). This phase does not need to solve a problem the codebase has already accepted elsewhere; it needs to not make the *existing* problem worse, which this candidate does not.
- *Persisted-state shape:* one new field, e.g. `pub last_validate_failure_commit_count: Option<u32>` with `#[serde(default)]`, following the exact convention every prior counter field uses (`consecutive_failures`, `infra_failures`, `preflight_retries`, `checkpoint_resumes` — all `[VERIFIED: crates/devflow-core/src/state.rs:44-113]`). `None` (absent from older persisted state) should be treated as "no prior failure recorded" — i.e., the very first observed failure in a resumed old-format run never triggers an incorrect reset or an incorrect accumulate; it simply becomes the new baseline.
- *Implementation complexity:* Low. The git query is copy-adjacent to existing, tested code; the comparison is simple integer arithmetic; no new external dependency; the reset/accumulate decision can still be expressed as a small pure function (`fn made_progress(previous: Option<u32>, current: u32) -> bool`) consulted by the I/O-performing caller, preserving the codebase's established pattern.

### Candidate B — findings/reason text comparison (CONTEXT.md's candidate (b))

**Mechanism.** Store the Validate agent's reported `reason`/`summary` text from the previous failure in a new `State` field; at the next failure, compare the new text to the stored text (exact match or some fuzzy comparison) and reset only if they differ.

**Why this candidate is materially weaker for this codebase specifically:**
- *The signal is frequently absent for the exact scenario this phase exists to fix.* `[VERIFIED: crates/devflow-core/src/prompt.rs:103-129]` — the Validate stage's prompt contract requires only `verdict: "pass"` or `verdict: "gaps"` for a `status: "success"` result; a `reason` field is only required on `status: "failed"`. A mid-arc "more waves remain" Validate result is a `status: "success", verdict: "gaps"` case — there is no contractual guarantee of any descriptive text to compare at all. Using this candidate would require first changing the Validate prompt contract to mandate a reason/summary even on a passing-task/gaps-verdict result — a scope expansion into prompt-contract design this phase's locked D-01/D-02 do not call for.
- *String comparison is a poor equality proxy either direction.* Two genuinely different problems can produce textually similar or identical boilerplate findings (false negative — fails to reset when it should); the same underlying problem can be described with slightly different wording across two agent runs (false positive — resets when it should not, weakening criterion 4 exactly the way candidate A's edge case does, but for the *common* case rather than an edge case).
- *Trust-boundary overlap with Phase 34's already-planned, separately-scoped work.* `[CITED: .planning/ROADMAP.md Phase 999.74 write-up]` — 999.74 (Phase 34, DOGFOOD-04) exists specifically because `classify_validate_outcome` currently trusts the agent's self-reported `verdict` field over independently-derived status, and ROADMAP.md explicitly frames that as an open trust-boundary question requiring "reading the Validate routing end to end" as its own dedicated investigation. Introducing a second heuristic that trusts agent-authored free text (`reason`/`summary`) as a safety-gate input, inside a phase that is explicitly scoped away from that broader trust-boundary work (per CONTEXT.md's Deferred Ideas), would pre-empt Phase 34's investigation with an untested assumption in the opposite direction.
- *Implementation complexity is comparable to Candidate A* (a new `Option<String>` field, `#[serde(default)]`) but the comparison logic is inherently fuzzier and harder to unit-test deterministically than an integer comparison.

### Recommendation: Candidate A (commit-count signal)

Use the commit-count comparison. It is objective (git-derived, not agent-self-reported), has a directly reusable implementation already proven elsewhere in this exact codebase (`evaluate_layer2`), requires no change to the Validate prompt contract, does not encroach on Phase 34's separately-scoped trust-boundary investigation, and its one honest weakness (a trivial-commit-producing stuck agent) is a pre-existing, already-accepted risk class in this codebase rather than a new one this phase would introduce. This also matches the roadmap's own stated lean `[CITED: .planning/ROADMAP.md:1747-1748]`: *"One candidate: reset on any loop-back where the prior Code stage's commit set differs from the one that produced the last Validate failure (genuine forward progress occurred)."*

**Concrete shape for the planner:**
1. New `State` field: `pub last_validate_failure_commit_count: Option<u32>`, `#[serde(default)]`, documented following the exact style of `consecutive_failures`'/`infra_failures`' doc comments (`state.rs:44-60`).
2. New helper (candidate home: `devflow-core::agent_result`, sibling to `evaluate_layer2`, or a new small function in the same module) computing the current commit count on the phase branch — extract the shared block from `evaluate_layer2` (`agent_result.rs:1862-1881`) rather than re-deriving it independently, to avoid the two counting mechanisms silently diverging over time.
3. New pure predicate (candidate home: `devflow-core::mode`, parallel to `transition_resets_consecutive_failures`), e.g. `pub fn consecutive_failures_made_progress(previous: Option<u32>, current: u32) -> bool { previous.is_none_or(|p| current > p) }` — deliberately treating "no prior recorded failure" (`None`) as "progress" so the very first failure in a phase, or the first failure after resuming an old-format state file, never accumulates incorrectly.
4. `handle_validate_outcome`'s existing increment block (`pipeline_outcomes.rs:264-270`) becomes: compute the current commit count (I/O), call the new pure predicate, and either reset-then-increment (to 1) or accumulate (as today) before the existing `should_gate` check — then update `last_validate_failure_commit_count` to the current count regardless of branch taken.

## Common Pitfalls

### Pitfall 1: Fixing 999.66 by touching `prepare_loop_back_to_code`
**What goes wrong:** A tempting first instinct is "the bug is that `prepare_loop_back_to_code` bypasses `transition()`, so make it call `transition()` (or duplicate its reset logic)."
**Why it happens:** CONTEXT.md's own domain description phrases the root cause as "the reset predicate is never consulted on the loop-back path," which reads as an invitation to wire that predicate into `prepare_loop_back_to_code`.
**How to avoid:** Confirmed by reading the function directly (`pipeline_gate.rs:130-158`) — it never touches `consecutive_failures` today, and it must not start, because the counter's *only* increment site is upstream, in `handle_validate_outcome`, which runs before `loop_back_to_code`/`prepare_loop_back_to_code` is ever called. The fix belongs entirely in `handle_validate_outcome`'s existing increment block, not in the state-mutating loop-back helper.
**Warning signs:** A diff that adds `consecutive_failures` logic to `pipeline_gate.rs` alongside `prepare_loop_back_to_code`'s existing `state.stage = Stage::Code;` line is very likely solving the wrong layer.

### Pitfall 2: Conflating 999.65's file-existence check with 999.66's reset signal
**What goes wrong:** Both fixes distinguish "mid-arc" from "something else" — it's tempting to reuse the `{N}-VERIFICATION.md`-existence boolean as the 999.66 reset signal too, since both feel like the same underlying "is this phase actually done being built" question.
**Why it happens:** Surface-level similarity, and it would look like a smaller diff.
**How to avoid:** `{N}-VERIFICATION.md` does not exist for the ENTIRE duration of a multi-wave phase until `/gsd-verify-work` finally runs at the end — it stays absent identically at wave 1, wave 2, and wave 3's *start*. Using it as the reset signal degenerates to "always reset because we're always mid-arc," which is exactly the forbidden "reset on every loop-back" bug in a different disguise. The two signals answer genuinely different questions (999.65: "which command should Code run next?" vs. 999.66: "did anything change since the last failure?") and must stay separate.
**Warning signs:** A single shared boolean/helper feeding both the `FixType` selection AND the `consecutive_failures` reset decision.

### Pitfall 3: `u32` overflow / saturating arithmetic
**What goes wrong:** A hand-rolled `+ 1` instead of `saturating_add(1)` on any of the touched counters.
**Why it happens:** Easy to miss when refactoring the existing increment block.
**How to avoid:** The existing code already documents why this matters (`pipeline_outcomes.rs:265-267`: "an unbounded loop could otherwise overflow it and wrap to 0, silently restoring the unreachable-ceiling bug in a slower form") — keep `saturating_add` on every touched counter, including any new commit-count field if it is ever incremented rather than replaced outright (in the recommended design it is *replaced* with the freshly-computed count each time, not incremented, so overflow is not actually a concern for that specific field — but the `consecutive_failures` field itself must keep its existing `saturating_add`).

### Pitfall 4: Backward compatibility for the new `State` field
**What goes wrong:** Deserializing an older, in-flight phase's `state.json` (written before this fix ships) fails, or silently produces a wrong reset decision.
**Why it happens:** Forgetting `#[serde(default)]` on the new field.
**How to avoid:** Every prior counter-style addition to `State` follows the identical pattern and has an explicit regression test for it (`state.rs:363-377` for `infra_failures`, `:429-444` for `monitor_pid`, `:465-479` for `session_id`, `:499-513` for `checkpoint_resumes`, `:531-547` for `yes_ship`, `:577-595` for the `stop_*` trio) — the new field needs the same `#[serde(default)]` attribute AND an equivalent "absent from JSON defaults to `None`" test.

## Code Examples

### The existing counter-increment site this phase's 999.66 fix modifies
`[VERIFIED: crates/devflow-cli/src/pipeline_outcomes.rs:264-270]`
```rust
if result == ValidateResult::Failed {
    // Now that the counter genuinely accumulates (18d), an unbounded
    // loop could otherwise overflow it and wrap to 0, silently
    // restoring the unreachable-ceiling bug in a slower form.
    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    workflow::save_state(state)?;
}
```

### The existing reset predicate this phase must NOT widen to accept I/O-derived data (mode.rs)
`[VERIFIED: crates/devflow-core/src/mode.rs:111-113]`
```rust
pub fn transition_resets_consecutive_failures(from: Stage, to: Stage) -> bool {
    !matches!((from, to), (Stage::Code, Stage::Validate))
}
```

### The existing test that already proves criterion 4's shape and should keep passing unchanged after the fix
`[VERIFIED: crates/devflow-cli/src/pipeline_outcomes.rs:1150-1201]` — `consecutive_failures_reaches_ceiling_across_cycles`: drives `MAX_CONSECUTIVE_FAILURES` real fail/Code→Validate cycles with PATH neutralized (no real agent spawn, so no real commits land between cycles) and asserts the counter reaches the ceiling and forces the gate. Because this existing test never produces new commits between failures, the recommended commit-count signal should still classify every cycle as "no progress" and accumulate exactly as it does today — this test is the direct regression guard for success criterion 4 and should require no behavioral change, only (if the fix touches shared setup) confirmation it still passes.

### Existing precedent for reading a git-derived commit count (to extract/mirror for 999.66)
`[VERIFIED: crates/devflow-core/src/agent_result.rs:1862-1881]`
```rust
let branch = format!("{}phase-{:02}", git_flow.feature_prefix, phase);
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

### Existing precedent for scanning `.planning/phases/` by numeric prefix (to mirror for 999.65)
`[VERIFIED: crates/devflow-core/src/agent_result.rs:2525-2541]`
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

### `FixType` and `fix_prompt` — where a new variant (if Claude's Discretion chooses one) would land
`[VERIFIED: crates/devflow-core/src/prompt.rs:34-41,278-286]`
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
A third arm producing `format!("/gsd-execute-phase {phase}")` (no `--gaps-only`) is the minimal addition if a new `FixType` variant is chosen over branching upstream of the call.

## State of the Art

Not applicable — this is an internal defect fix against code this same codebase wrote across phases 17/18/23, not an adoption of an external best practice. No "old approach vs. current approach" axis exists outside this codebase's own history, which is already narrated inline in the source comments cited above (18d's original bug, 18-04's fix, this phase's refinement of that fix for the multi-wave case).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | GSD's plan-based `/gsd-execute-phase {N}` and `--gaps-only` executions normally produce substantive (non-trivial) commits per plan/wave, making Candidate A's "trivial commit" weakness a low-probability edge case rather than a common one. | `## D-03 Recommendation`, Candidate A tradeoffs | If wrong (agents routinely produce near-empty commits), Candidate A's safety-gate weakening would be more frequent than assessed here, and the planner should consider pairing the commit-count signal with a minimum-lines-changed or minimum-file-count threshold as an additional guard — not swap to Candidate B, whose weaknesses are structural rather than probabilistic. |
| A2 | No production code path other than the three D-02-scoped call sites and the one out-of-scope Ship call site currently constructs a `FixType::GapsOnly`/`AuditFix` value — i.e., D-02's "3 in-scope + 1 out-of-scope" is the complete set. | `## Architecture Patterns` Pattern 1 | Confirmed via direct grep of both `pipeline_outcomes.rs` reads in this session — all 4 `FixType::GapsOnly` constructions were visually located and line-matched against CONTEXT.md's citations with zero drift. Low residual risk; flagged only because a workspace-wide grep for `FixType::` across every crate was not separately run in this session. |

**If confirming A2 further:** a workspace-wide `rg -n "FixType::" crates/` at plan time would close this residual risk at near-zero cost and is recommended as a first planning-time verification step, not a re-research need.

## Open Questions

1. **Where exactly should the new commit-count helper and the new pure predicate live (which module)?**
   - What we know: `evaluate_layer2`'s block is the mechanism to reuse; `mode.rs` is the established home for pure gate-affecting predicates.
   - What's unclear: whether to extract a shared `fn commits_on_branch(...)` callable from both `evaluate_layer2` and the new 999.66 site (avoiding duplication) or to accept a small amount of duplication for module-boundary cleanliness (`agent_result.rs` is already very large — `evaluate_layer2` starts around line 1850 in a multi-thousand-line file).
   - Recommendation: this is exactly the kind of naming/placement decision CONTEXT.md's "Claude's Discretion" section reserves for the planner/executor; the D-03 substance (which signal, why) is settled above regardless of where the helper physically lives.

2. **Does the new `FixType` variant (if chosen) need its own dedicated unit test mirroring `fix_prompts_select_the_right_command` (`prompt.rs:492-497`)?**
   - What we know: every existing `FixType` variant has a direct test asserting its exact command string.
   - What's unclear: nothing structurally — this is a straightforward "yes," flagged only so the planner's test-writing task doesn't miss it as an omission rather than a deliberate choice.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust's built-in `cargo test`, inline `#[cfg(test)] mod tests` colocated with each modified source file — no separate test directory or config file for this workspace's Rust crates |
| Config file | none — the workspace `Cargo.toml` plus each crate's own `Cargo.toml`; test behavior is governed by `scripts/check.sh` (`[VERIFIED: scripts/check.sh:1-53]`), not a pytest/jest-style test-config file |
| Quick run command | `cargo test -p devflow --lib pipeline_outcomes::` (touches `handle_validate_outcome`'s tests) and `cargo test -p devflow-core --lib mode::` / `state::` as needed — the CLI crate's package name is `devflow`, not `devflow-cli` `[VERIFIED: crates/devflow-cli/Cargo.toml:2]`, per this repo's own CLAUDE.md verification-habits note |
| Full suite command | `scripts/check.sh all` (fmt + clippy `-D warnings` + `cargo test --workspace --no-fail-fast`) `[VERIFIED: scripts/check.sh:26-53]` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DOGFOOD-01 (999.65, criterion 1) | Mid-arc Validate failure (no `{N}-VERIFICATION.md`) issues a plain `/gsd-execute-phase {N}` fix prompt, not `--gaps-only` | unit | `cargo test -p devflow --lib pipeline_outcomes::mid_arc_loop_back_issues_plain_command -- --exact` (new test, name illustrative) | ❌ Wave 0 — new test |
| DOGFOOD-01 (999.65, criterion 2) | Genuine-gaps Validate failure (`{N}-VERIFICATION.md` exists with findings) still issues `--gaps-only` | unit | `cargo test -p devflow --lib pipeline_outcomes::genuine_gaps_loop_back_still_issues_gaps_only -- --exact` (new test) | ❌ Wave 0 — new test |
| DOGFOOD-02 (999.66, criterion 3) | 3+ healthy wave transitions (new commits landing between cycles) do not false-gate | unit/integration | new variant of `consecutive_failures_reaches_ceiling_across_cycles` (`pipeline_outcomes.rs:1150-1201`) that commits real work between cycles and asserts the gate does NOT fire at cycle 3 | ❌ Wave 0 — new test, adapted from existing pattern |
| DOGFOOD-02 (999.66, criterion 4) | Same unresolved problem (no new commits between cycles) still reaches the ceiling and gates | unit | `consecutive_failures_reaches_ceiling_across_cycles` (`pipeline_outcomes.rs:1150-1201`) — existing test, already exercises zero-commit cycles via PATH-neutralized `launch_stage` failures; should continue passing unchanged as the direct regression guard | ✅ Exists — regression guard, verify it still passes post-fix |
| — | New `State` field backward-compat (absent-from-JSON default) | unit | new test mirroring `infra_failures_absent_from_json_defaults_to_zero` (`state.rs:363-377`) | ❌ Wave 0 — new test |

### Sampling Rate
- **Per task commit:** the relevant crate-scoped quick command above (e.g. `cargo test -p devflow --lib pipeline_outcomes::`)
- **Per wave merge:** `cargo test --workspace --no-fail-fast`
- **Phase gate:** `scripts/check.sh all` green before `/gsd-verify-work` — matches this repo's own documented CI-parity contract

### Wave 0 Gaps
- [ ] A helper to simulate "Code produced N new commits since the last Validate failure" inside a test — likely a small test-support addition (create a phase branch, commit, run the cycle, commit again) alongside the existing `test_support::*` helpers already used throughout `pipeline_outcomes.rs`'s test module (e.g. `init_repo`, `agent_free_git_only_path_dir`).
- [ ] The four new unit tests named in the Phase Requirements → Test Map above.
- [ ] No new test framework install needed — `cargo test` is already the workspace's only test runner.

## Security Domain

### Applicable ASVS Categories

This phase is a local, single-operator CLI's internal state-machine correctness fix — not a network-facing service, so most ASVS categories (authentication, session management, access control in the multi-user sense) do not apply. The relevant category is input validation of persisted/derived state.

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Single-operator local CLI; no auth boundary crossed by this change |
| V3 Session Management | no | Not applicable — `session_id` in `State` is an unrelated pre-existing field (Claude session resume, phase 28) this fix does not touch |
| V4 Access Control | no | No multi-principal access boundary in scope |
| V5 Input Validation | yes | `serde`'s `#[serde(default)]` + strong typing on the new `State` field is the existing, established control for "state.json was hand-edited or written by an older/different binary" — every prior counter field already relies on this and has a direct round-trip + absent-defaults test pair (see `## Common Pitfalls` Pitfall 4) |
| V6 Cryptography | no | Not applicable |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| An operator (or a bug) hand-edits `.devflow/state.json` to set an implausible `last_validate_failure_commit_count` (e.g., a very large number), permanently defeating the reset-vs-accumulate distinction | Tampering | Low severity given the existing threat model — `.devflow/state.json` is already fully trusted, locally-writable state with no integrity check on any of its other counters (`consecutive_failures` itself could be hand-edited to `0` today to defeat `MAX_CONSECUTIVE_FAILURES` entirely); the new field introduces no *new* class of risk beyond what already exists for every other counter in `State`. No new mitigation is warranted beyond the existing trust model — flagged for completeness, not as an action item. |
| A stuck-but-still-committing Code agent defeats the safety gate by making trivial no-op commits each cycle | Repudiation / gate bypass | Documented explicitly in `## D-03 Recommendation` (Candidate A's known weakness) and `## Assumptions Log` A1 — accepted risk, not mitigated in this phase; already exists in an adjacent form via `evaluate_layer2`'s own "no work done" gate, which any single trivial commit also already defeats today. If this proves to matter in practice, a follow-up backlog entry (not this phase) should consider a minimum-substance threshold (lines changed, files touched) rather than a bare commit count. |

## Sources

### Primary (HIGH confidence — read directly this session)
- `crates/devflow-cli/src/pipeline_outcomes.rs` (full file through line 1251, covering all functions and tests relevant to this phase) — `classify_validate_outcome`, `ValidateOutcome`/`ValidateResult`, `handle_validate_outcome`, `handle_ship_outcome`, and the existing regression test suite
- `crates/devflow-cli/src/pipeline_gate.rs` (through line 1284) — `transition`, `loop_back_to_code`, `prepare_loop_back_to_code`, `finish_workflow`
- `crates/devflow-core/src/mode.rs` (full file) — `MAX_CONSECUTIVE_FAILURES`, `transition_resets_consecutive_failures`, `Mode::should_gate`
- `crates/devflow-core/src/prompt.rs` (full file) — `FixType`, `fix_prompt`, `validate_stage_prompt` (confirms the `verdict`-vs-`reason` contract used in Candidate B's rejection)
- `crates/devflow-core/src/state.rs` (full file) — `State` struct, every existing `#[serde(default)]` counter field and its backward-compat test pattern
- `crates/devflow-core/src/agent_result.rs` (`evaluate_layer2` at :1850-1934, `phase_review_path` at :2525-2541) — the two reusable precedents this recommendation is built on
- `.planning/ROADMAP.md` (lines 1-70, 1700-1797) — Phase 33's locked success criteria, and the 999.65/999.66 backlog write-ups including the roadmap's own stated D-03 lean
- `.planning/phases/33-.../33-CONTEXT.md`, `.planning/REQUIREMENTS.md`, `.planning/STATE.md` — phase scope, requirement IDs, project history
- `scripts/check.sh` (full file) — the workspace's canonical test/lint commands
- `crates/devflow-cli/Cargo.toml`, `crates/devflow-core/Cargo.toml` (package name lines) — confirms `devflow-cli`'s package name is `devflow`

No Context7/web search was performed — this phase's REQUIREMENTS.md explicitly states no domain research was run for this milestone, and confirmed during this session: the entire scope is internal, already-shipped Rust code with no third-party library or external API involved.

## Metadata

**Confidence breakdown:**
- Standard stack: N/A — no new external dependency
- Architecture / D-01/D-02 mechanism: HIGH — every cited line number was read directly this session and matched CONTEXT.md's citations exactly, with zero drift
- D-03 recommendation: HIGH — both candidates evaluated directly against real source (`transition()`, `prepare_loop_back_to_code`, `transition_resets_consecutive_failures`, `handle_validate_outcome`'s increment site, `validate_stage_prompt`'s contract), with a concrete, precedent-grounded implementation path for the recommended candidate
- Pitfalls: HIGH — derived directly from tracing the real control flow, not generic Rust-domain knowledge

**Research date:** 2026-08-04
**Valid until:** No natural expiry — this is a fix against this repository's own code, not a moving external target. Re-verify line numbers only if `pipeline_outcomes.rs`, `pipeline_gate.rs`, `mode.rs`, `prompt.rs`, or `state.rs` are edited by any other phase before Phase 33 executes.
