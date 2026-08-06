# Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles (999.65 + 999.66) - Context

**Gathered:** 2026-08-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Two structural defects in the Validate→Code loop-back mechanism (`crates/devflow-cli/src/pipeline_outcomes.rs`, `crates/devflow-cli/src/pipeline_gate.rs`, `crates/devflow-core/src/mode.rs`), both confirmed live in the Phase 29 dogfood run and both blocking any unattended, 3+ wave `devflow start` phase from completing its Code↔Validate loop:

- **999.65** — every Validate→Code loop-back unconditionally issues `/gsd-execute-phase {N} --gaps-only`, which matches zero plans (and gates unresolvably) on a phase that is mid-arc (no `{N}-VERIFICATION.md` yet, waves remaining) rather than one with genuine defects in already-built work.
- **999.66** — `consecutive_failures` is monotonic across a loop-back because `prepare_loop_back_to_code` (`pipeline_gate.rs:130`) sets `state.stage = Stage::Code` directly and never calls `transition()`, so `transition_resets_consecutive_failures` (`mode.rs:111`) is never consulted on the loop-back path. Every wave transition that isn't a first-try pass increments the counter, false-gating a healthy 3+ wave phase at wave 3.

This phase clarifies the two distinguishing signals the fixes need — not the wider stream-json/Validate-trust-boundary work (999.73/999.74), which is Phase 34's separate scope by explicit ROADMAP.md decision.

</domain>

<decisions>
## Implementation Decisions

### 999.65 — mid-arc vs. genuine-gaps signal
- **D-01:** The loop-back distinguishes "Validate correctly found the phase mid-arc/unbuilt" from "Validate found real defects in already-built work" using **file existence of `{N}-VERIFICATION.md` alone** — no `{N}-VERIFICATION.md` → issue plain `/gsd-execute-phase {N}`; file exists → issue `--gaps-only`, unchanged. This matches the phase's own success criteria literally and needs no parsing of VERIFICATION.md's internal finding-type breakdown. — **Reversibility:** reversible — a pure decision-function change, no persisted-state shape involved.
- **D-02:** The fix touches only the **3 call sites inside `handle_validate_outcome`** (`pipeline_outcomes.rs:255`, `:285`, `:292` — the Ambiguous-gate loop-back, the consecutive-failure gate loop-back, and the plain Failed loop-back). The 4th `FixType::GapsOnly` call site, inside `handle_ship_outcome` (`:321`, a human rejecting a completed Ship and looping back to Code), is explicitly **out of scope** — by the time Ship runs the phase is by definition not mid-arc, so `--gaps-only` is already correct there. — **Reversibility:** reversible — narrowing scope now doesn't block widening it later if a real Ship-loop-back defect turns up.

### 999.66 — forward-progress signal for the consecutive_failures reset
- **D-03:** The signal that distinguishes "genuine forward progress" (reset the counter) from "same problem again" (keep counting toward `MAX_CONSECUTIVE_FAILURES`) is **deferred to the phase researcher** rather than locked here. Two candidates were surfaced and neither was picked outright:
  - Track the Code-stage commit set/HEAD at the moment Validate fails; reset only if Code has produced new commits since then (the roadmap's own candidate direction).
  - Compare Validate's reported findings/reason text across loop-backs; reset only if the findings differ from the immediately preceding failure.
  The researcher should lay out both (and any other viable option) against the actual `transition()` / `prepare_loop_back_to_code` code paths, name the concrete tradeoffs, and recommend one before planning locks it in. **Do not** default to "reset on every loop-back" — that reintroduces 18d's original unreachable-ceiling bug in a different form (this is a hard constraint from the backlog entry, not a preference). — **Reversibility:** one-way for whichever heuristic ships — it likely requires a new persisted `State` field (there is currently no wave counter and no "commit set at last Validate failure" field), and changing the persisted-state shape after release means an older in-flight phase's saved state won't carry the new field, so the reset predicate must have a safe default for its absence.

### Claude's Discretion
- Exact shape of any new `State` field(s) needed to support D-03's eventual heuristic (naming, type, serde defaulting for older persisted state).
- Whether `handle_validate_outcome`'s `FixType` enum needs a new variant (e.g., distinct from `GapsOnly`) versus keeping `FixType::GapsOnly`/`AuditFix` as-is and doing the mid-arc/gaps branch upstream of the `loop_back_to_code` call.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and success criteria
- `.planning/ROADMAP.md` — `### Phase 33: Loop-Back Correctness for Multi-Wave Validate→Code Cycles (999.65 + 999.66)` section — the 4 locked success criteria this phase must satisfy.
- `.planning/ROADMAP.md` — `### Phase 999.65: The Validate→Code Loop-Back Issues an Impossible Command on a Mid-Arc Phase (BACKLOG)` — full defect writeup, live Phase 29 evidence.
- `.planning/ROADMAP.md` — `### Phase 999.66: consecutive_failures Accumulates on Healthy Multi-Wave Progress, Not Just Repeated Failure (BACKLOG)` — full defect writeup, explicit warning against the naive "reset on every loop-back" fix.
- `.planning/REQUIREMENTS.md` — DOGFOOD-01, DOGFOOD-02 (this phase's two requirements).
- `.planning/PROJECT.md` — `## Current Milestone: v2.4.0 Resume Unattended Dogfooding` — why this milestone exists (Phase 29 dogfood + Phase 31 planning findings, no new regressions).

### Source of the two defects
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `classify_validate_outcome` (`:203`), `ValidateOutcome`/`ValidateResult` (`:160`, `:227`), `handle_validate_outcome` (`:235`) — all 3 in-scope `FixType::GapsOnly` call sites live here (`:255`, `:285`, `:292`); `handle_ship_outcome` (`:306`) has the 4th, out-of-scope, call site (`:321`).
- `crates/devflow-cli/src/pipeline_gate.rs` — `prepare_loop_back_to_code` (`:130`) — the state-mutating half of the loop-back; sets `state.stage = Stage::Code` directly, bypassing `transition()` entirely (why 999.66 exists).
- `crates/devflow-core/src/mode.rs` — `transition_resets_consecutive_failures` (`:111`), `MAX_CONSECUTIVE_FAILURES` (`:18`), `Mode::should_gate` (`:132`) — the reset predicate and gate threshold this phase must not defeat.
- `crates/devflow-core/src/prompt.rs` — `FixType` enum (`:36`), `fix_prompt` (`:278`) — where `--gaps-only` vs. plain `/gsd-execute-phase {N}` command strings are selected.
- `crates/devflow-core/src/state.rs` — `State` struct (`:34`) — current persisted fields; confirms no wave counter and no "commit set at last Validate failure" field exists today, informing D-03's researcher hand-off.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Mode::should_gate` / `MAX_CONSECUTIVE_FAILURES` — the existing 3-strike gate logic to preserve exactly (success criterion 4); the fix narrows the false positive, it does not touch this threshold or its Supervise/Auto branching.
- `ValidateResult`'s two-variant enum pattern (`Passed`/`Failed`, deliberately exhaustive-match-friendly per WR-03/18-fix) — the established style for adding any new outcome discrimination without an `unreachable!()` arm.

### Established Patterns
- `transition_resets_consecutive_failures` is a pure predicate function consulted by `transition()` — the established pattern for gate-affecting decisions is a small pure function, not inline logic at each call site. Any 999.66 fix should follow this shape rather than special-casing inside `prepare_loop_back_to_code`.
- `State` fields that persist across `devflow advance` invocations already use `#[serde(default)]` for backward compatibility with older saved state (see `consecutive_failures`, `infra_failures`, `preflight_retries`) — the same convention applies to any new field D-03's fix requires.

### Integration Points
- `prepare_loop_back_to_code` is the single choke point both defects pass through: it decides the destination stage (never via `transition()`, 999.66's root cause) and returns the fix prompt via `prompt::fix_prompt` (999.65's root cause is upstream of this, in which `FixType` the caller passes).

</code_context>

<specifics>
## Specific Ideas

No UI/UX specifics — this is a backend correctness fix to the pipeline state machine. Codebase scouting during discussion (not user free-text) established the concrete call sites and existing patterns recorded in `<code_context>` above.

</specifics>

<deferred>
## Deferred Ideas

- 999.73 (widen stream-json launch path beyond `Stage::Code`) and 999.74 (`classify_validate_outcome` trusting self-reported `verdict`) — already split into Phase 34 by an explicit ROADMAP.md decision predating this discussion; not re-litigated here.
- Whether `handle_ship_outcome`'s Ship→Code loop-back (`pipeline_outcomes.rs:321`) has its own latent defect — out of scope for this phase (D-02), but flagged in case a future dogfood run surfaces a real problem there.

### Reviewed Todos (not folded)
None — `todo.match-phase 33` returned zero matches.

</deferred>

---

*Phase: 33-Loop-Back Correctness for Multi-Wave Validate→Code Cycles*
*Context gathered: 2026-08-04*
