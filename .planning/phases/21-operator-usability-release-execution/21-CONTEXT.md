# Phase 21: Operator Legibility & Observability - Context

**Gathered:** 2026-07-23 (headless discuss) · **Scope recut:** 2026-07-23 (operator decision, this session)
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 21 makes DevFlow's operator surface **legible** and its self-reported
state **trustworthy** — every unit is single-writer, operator-facing, reversible
or detection-only, and fully testable without any irreversible side effect. It
deliberately carries **no** release-execution or branching-model work.

> **Scope recut (operator decision, 2026-07-23).** The headless discuss-phase
> proposed 21a discoverability + `--base` (999.28) + release-cut executor
> (999.25). Both of the latter were **removed** at operator review:
> - **999.25 (release executor) → its own dedicated phase.** It drives
>   irreversible operations (crates.io publish, signed tag, merge to main),
>   its own dossier says it "needs its own discuss-phase design pass on
>   failure/rollback semantics," and it cannot be exercised inside a dogfood
>   loop without a real publish. It deserves a focused, *interactive* discuss —
>   not a bundled, headless one.
> - **999.28 (`--base`) → Phase 22.** Its only payoff is stacking a phase onto
>   an *unmerged* predecessor; under the current sequential-supervised cadence,
>   merging a phase to `develop` gives the next phase its work for free. The
>   value is concurrency/stacking — Phase 22's domain — so all of `--base`
>   moves there rather than being split across phases.
>
> The phase was renamed **"Operator Usability & Release Execution" →
> "Operator Legibility & Observability"** to match.

The ROADMAP goal was left `[To be planned]` on purpose (scaffold commit
`56a1835`). This CONTEXT therefore also **records the operator-confirmed scope**.
The unit set below is operator-decided; it did **not** go through a
`/gsd-review-backlog` promotion and REQUIREMENTS.md carries no REQ-IDs, so
confirm sizing at plan/review time.

**In scope (operator-facing legibility / observability):**

- **21a — Operator discoverability (999.3).** `devflow gate show` for truncated
  gate reasons, surface rate-limit reset times out of raw agent JSON, in-stage
  progress in `status`, and make recovery verbs (`advance`, `resume`)
  discoverable from a stuck state. UX only — no behavioral change. Bundles four
  distinct gaps; the planner may split them. **Sequence first** (lowest risk,
  unblocks nothing).
- **21b — Doctor reconciliation for planning-doc staleness (999.14).**
  `devflow doctor` already reconciles phase state against events/PIDs/gates/
  branches (18a), but nothing checks whether `ROADMAP.md`/`STATE.md`'s own
  **narrative** still matches reality after a manual, out-of-band merge/tag/
  publish. This session hit exactly that bug (STATE claimed Phase 18
  unreleased after v1.5.0 shipped). **Detection-only** — flag stale version
  claims against git tags; do **not** auto-correct prose.
- **21c — sequentagent's untracked second process (999.2).** One
  `phase-N-agent-pid` file per phase; `sequentagent` runs a *second* agent that
  "does not participate in the stage machine" (`parallel.rs`) and is left
  unrecorded. The monitor half of the original item already shipped in v1.5.0
  (18b, `State.monitor_pid`), so this is **narrowed** to sequentagent's orphaned
  second process only — re-scope precisely at plan time.

**Optional / stretch (include only if 21a–c leave capacity):**

- **21d — `ChangelogAppend` real content (999.5).** Every generated entry reads
  `- Released phase via DevFlow.` (`ship.rs:431`). Cosmetic, deferred three
  times already, and **blocked on choosing a content source** (SUMMARY.md
  extraction vs plan diffs) — which is why it is stretch-only, not a committed
  unit.

**Explicitly OUT of scope:** 999.25 release executor (own dedicated phase),
999.28 `--base` (Phase 22), plus Phase 22 concurrency (999.4, 999.26) and
Phase 23 test/CI (999.15/17/18/19/20/22).
</domain>

<decisions>
## Implementation Decisions

Ratings follow `gsd-core/references/planner-reversibility.md`.

### Scope (operator-decided this session)

- **D-01:** Remove 999.25 (release executor) and 999.28 (`--base`) from Phase 21
  — see the boundary note for rationale. 999.25 → its own phase with an
  interactive discuss; 999.28 → Phase 22. — **Reversibility:** reversible (a
  scoping decision; nothing built yet).
- **D-02:** Phase theme is **operator legibility & observability**; renamed from
  "Operator Usability & Release Execution." All units must remain single-writer,
  reversible/detection-only, and dogfood-testable (no irreversible side effects).

### 21a — Operator discoverability

- **D-03:** Purely **additive UX** surfacing — `gate show`, rate-limit reset
  time in human output, in-stage progress in `status`, recovery-verb hints from
  a stuck state. No behavioral/correctness change to the pipeline. Sequence it
  first. — **Reversibility:** reversible.

### 21b — Doctor planning-doc reconciliation

- **D-04:** **Detection-only.** Add a `doctor` check that compares
  `ROADMAP.md`/`STATE.md` version/outcome claims against git tags (and, where
  cheap, published state) and **flags** drift. Do **not** auto-edit prose —
  same discipline 18a's reconciliation already follows. — **Reversibility:**
  reversible.
- **D-05:** Integrate as a new `Check` in the existing `doctor` path
  (`commands.rs:1121`, JSON body at `:1866`) so human and `--json` output stay
  consistent; do not fork a second reporter.

### 21c — sequentagent second-process tracking

- **D-06:** **Re-scope before planning.** The "monitor unrecorded" half shipped
  in v1.5.0 (18b). Remaining scope is *only* `sequentagent`'s second agent,
  which runs off the stage machine (`parallel.rs`) and has no pid record. Define
  what "tracked" means for a non-stage-machine handoff (a second pid file? a
  `sequentagent`-specific record?) as the first plan step. — **Reversibility:**
  reversible.

### 21d — ChangelogAppend content (stretch)

- **D-07:** Stretch-only. Blocked on choosing a per-phase content source; if
  pulled in, that choice (SUMMARY.md extraction vs plan-diff summary) is a
  design decision the planner must make explicit, not assume.

### Claude's Discretion
- Exact CLI flag surface for `gate show` (positional vs `--phase`), progress
  representation in `status`, and whether 21a ships as one plan or splits by
  sub-gap — planner's call.
- Precise re-scope of 999.2 and whether 21d is folded in at all.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase-21 unit sources (backlog dossiers — read before scoping)
- `.planning/phases/999.3-cli-operator-discoverability/CONTEXT.md` — 21a: the
  four discoverability gaps and why they are UX, not correctness.
- `.planning/phases/999.14-doctor-planning-doc-reconciliation/CONTEXT.md` —
  21b: the staleness-detection gap and the "detect, don't auto-correct" scope.
- `.planning/phases/999.2-phase-process-tracking-model/CONTEXT.md` — 21c: the
  two-processes-per-phase framing; note the monitor half is already shipped.
- `.planning/phases/999.5-changelog-placeholder-content/CONTEXT.md` — optional
  21d, and the open "where does real per-phase content come from" question.

### Code the units extend (not rediscover)
- `crates/devflow-cli/src/commands.rs:1121` (`doctor`) + `:1866`
  (`doctor_json_body`) — 21b adds a reconciliation `Check` here.
- `crates/devflow-cli/src/parallel.rs` (sequentagent handoff; comments at
  `:5`/`:181`/`:192` state it does not participate in the stage machine) and
  `agent_result::agent_pid_path` (used at `commands.rs:889`) — 21c's surface.
- `crates/devflow-core/src/ship.rs:431` — the `ChangelogAppend` placeholder 21d
  would replace.
- `devflow gate` / `devflow status` output paths (`commands.rs`, `main.rs`) —
  21a surfacing.

### Scope-fence refs (what is NOT this phase)
- `.planning/phases/999.25-release-cut-executor/CONTEXT.md` — its own phase.
- `.planning/phases/999.28-explicit-base-branch-override/CONTEXT.md` — Phase 22.
- ROADMAP.md §"Phase 22/23".
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `doctor`'s existing `Check` list + `doctor_json_body` (`commands.rs:1121`/
  `:1866`) — 21b appends to these; single-object JSON contract already hardened
  (18 WR-01). Do not fork a second reporter.
- 18b's `State.monitor_pid` (shipped v1.5.0) — 21c builds on the same
  process-record model, extended to sequentagent's second agent.
- `agent_result::agent_pid_path` (`commands.rs:889`) — the per-phase pid-file
  convention 21c must reconcile against for the second process.

### Established Patterns
- Reconciliation = **detect and report, never auto-correct** (18a). 21b follows
  it exactly (D-04).
- Command surface lives in `crates/devflow-cli/src/main.rs` + `commands.rs`;
  21a/21b extend the same dispatch.
- `sequentagent` deliberately sits outside the stage machine (`parallel.rs`) —
  21c must not force it into the stage model, only give its second process a
  record.

### Integration Points
- `devflow doctor` (`commands.rs:1121`) ← 21b reconciliation check.
- `devflow gate` / `devflow status` output ← 21a surfacing.
- `sequentagent` launch path (`parallel.rs`) ← 21c second-process record.
- `ChangelogAppend` (`ship.rs:431`) ← optional 21d.
</code_context>

<specifics>
## Specific Ideas

- The motivating bug for 21b is this session's own: `STATE.md`/`ROADMAP.md`
  narrative drifting from git reality after a manual release — a legibility
  failure `doctor` should catch, mirroring `[[project-gsd-ui-gate-cli-false-positive]]`
  and the staleness class in `[[project-gsd-execute-devflow-quirks]]`.
- 21a and 21b share a theme: both make devflow's *own* state legible to the
  operator (one via richer live output, one via drift detection).
</specifics>

<deferred>
## Deferred Ideas

- **999.25 — Release-cut executor → its own dedicated phase.** Irreversible
  (crates.io publish, signed tag, merge to main); its dossier requires its own
  discuss-phase on rollback semantics (tag-lands-publish-fails; core-publishes-
  cli-does-not) and a design for testing without a real publish. Do not fold
  back into a legibility phase.
- **999.28 — `--base` branch override → Phase 22.** Value is concurrency/
  stacking (build phase N on an unmerged N-1); the whole feature (start + the
  `parallel` shared-base derivation) belongs together in the concurrency phase,
  not split.
- **Phase 22 concurrency:** 999.4 (version-tag contention), 999.26 (`parallel`
  object-store race), the concurrency half of 999.2.
- **Phase 23 test/CI:** 999.15/17/18/19/20/22.
</deferred>

---

*Phase: 21-operator-usability-release-execution*
*Context gathered: 2026-07-23 (headless) · recut 2026-07-23 (operator)*
