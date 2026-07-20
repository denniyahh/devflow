# Phase 17: Pipeline Dogfood Follow-Up - Context

**Gathered:** 2026-07-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Make DevFlow's own completion signals trustworthy: no stage advances on an
unclassified outcome, every agent outcome carries a typed classification with
a defined policy, readiness failures surface before agent time is spent, and
the event stream identifies the binary that produced it.

Four scope units, each traced to a specific Phase 16 dogfood observation —
full evidence in `17-DOGFOOD-RETROSPECTIVE.md` (MUST-read):

- **17a** — `Unknown` completion must not auto-advance
- **17b** — typed agent outcomes + deterministic retry policy
- **17c** — preflight readiness gate before stage launch
- **17d** — build provenance in `workflow_started` + stale-build detection

Explicitly out of scope: `devflow doctor` state/event reconciliation and the
WR-03 test fix (both deferred to Phase 18 as 18d/18e on 2026-07-18); Hermes
support (Phase 18); full `main.rs` orchestration extraction (see Deferred).

Retrospective acceptance criterion 1 (failed Merge leaves branch intact,
blocks terminal hooks, opens a Ship gate) is ALREADY covered by a Phase 16
regression test — verify it against final HEAD, do not re-plan it. The
terminal-Ship alarm was traced to a stale executable, not a live regression.

</domain>

<decisions>
## Implementation Decisions

### 17a — Unknown-outcome policy

- **D-01 (root defect):** `main.rs:854` classifies only `Failed |
  RateLimited` as failure, so `Unknown` falls through to the success arm at
  `main.rs:871` (whose comment states the behavior outright) and
  `Stage::Code => transition(..., Stage::Validate)` fires unconditionally.
  Broader than the retrospective recorded: `evaluate_layer3`
  (`agent_result.rs:610-620`) returns `Unknown` for the zero-commit "process
  gone, no commits" branch too, so a vanished agent that produced nothing
  also advances.

- **D-02 (fix locus): split at Layer 3 into typed outcomes.** `evaluate_layer3`
  returns distinct outcomes rather than one `Unknown`. Type-driven: the
  exhaustive match in `advance()` forces every future stage/outcome pair to be
  handled, which prevents this regression class rather than re-patching it.
  Composes with 17b's taxonomy.

- **D-03 (zero-commit policy): three-way, not binary.** Zero commits is NOT
  inherently failure — the originating incident was an external check that
  legitimately produced no code. Policy:
  1. zero commits + declared external post-condition **passed** → advance
  2. zero commits + declared external post-condition **failed** → fail
     (already works today)
  3. zero commits + **no declaration at all** → genuinely ambiguous what the
     stage did → treat as failure, notify human for review

- **D-04 (commits-present policy):** post-condition if declared, else gate for
  explicit human approval. Matches retrospective AC-3 literally and reuses
  Phase 16's Layer 0 rather than inventing a second mechanism.

- **D-05 (Layer 0 must be extended — REQUIRED for D-03):** 16a's Layer 0
  exists (`agent_result.rs:627-690`) and is the right mechanism, but has two
  gaps that make D-03 unimplementable as-is:
  1. **Code-stage only** — `agent_result.rs:638` returns `None` unless
     `state.stage == Stage::Code`. Conflicts with D-06.
  2. **Passing probes are not affirmative evidence** — the docstring is
     explicit (lines 629-631): success "defers to the existing Layer 1/2/3
     cascade", and the code only maps failures. A probe can veto, never vouch.
     Consequence: an external-only stage with zero commits still cannot
     succeed cleanly — it falls to Layer 2's Plan|Code commit gate or Layer 3
     `Unknown`. This is exactly the originating incident.

  Phase 17 lifts the stage restriction and lets a passing declared probe count
  as affirmative completion evidence. The operator-approval mechanism
  (`TRUST_EXTERNAL_VERIFY_ENV` holding a reviewed JSON command array, with
  mismatch detection in both directions) is UNCHANGED — it is the security
  property that makes Layer 0 trustworthy and must not be relaxed.

- **D-06 (stage scope): every stage.** Define, Plan, Code, Ship all get the
  non-advance rule. Validate is already fail-safe via 13-05's verdict gating
  so is unaffected in practice, but a uniform rule leaves no stage-shaped hole.

### 17b — Outcome taxonomy + retry policy

- **D-07 (new outcomes):** add `ResourceKilled` (exit 137 — currently unhandled;
  `rg "137"` returns nothing workspace-wide) and `AgentUnavailable`.
  `RateLimited` and `Unknown` already exist (`agent_result.rs:41-50`), so the
  retrospective's proposed five-outcome set is three-quarters present.

- **D-08 (failure budget): separate counters.** Infrastructure outcomes
  (rate-limited, OOM-killed) do NOT increment `consecutive_failures`, the
  counter driving gate→abort. They get their own counter with its own ceiling.
  Rationale: spending the abort budget on conditions the agent never
  controlled aborts phases whose work was fine — the same false-signal family
  this phase exists to fix.

- **D-09 (retry): auto-resume `rate_limited` only.** Rate limits already have
  resume machinery (`cron-instructions-NN.json`); extending it is cheap and
  the recovery is unambiguous. Every other outcome gates — an OOM kill or a
  missing binary needs a human to change something, and auto-retrying a
  workload that will always exhaust memory burns agent time unobserved.

- **D-10 (evidence): structured record on every terminal decision.** Replaces
  `reason: null` on success events. Emit which layer decided (0/1/2/3), the
  outcome, and the detail as FIELDS, not prose — machine-readable for 18d's
  reconciliation and greppable in `events.jsonl`. Follow the existing schema-v1
  idiom in `events.rs`; do not invent a new store.

- **D-11 (policy locus): hardcoded table, no config knobs.** An exhaustive
  match in `devflow-core` so adding an outcome forces declaring its policy at
  compile time. Phase 16's D-03 admits `devflow.toml` knobs only where one is
  warranted; a safety-critical policy table is the opposite of a knob — a
  configurable fail-closed guarantee is not a guarantee.

- **D-12 (extraction, testability-driven):** the outcome→policy mapping lands
  in `devflow-core` as a PURE function taking typed outcomes and returning an
  action enum — no I/O, no `CliError`. This is where the policy belongs on its
  own merits and makes the fail-closed paths unit-testable without spawning an
  agent. It follows the 13-01 precedent (`prepare_loop_back_to_code` split out
  of `loop_back_to_code` for exactly this reason). Shrinking `advance()` is a
  side effect, not the goal. See Deferred for what is explicitly NOT extracted.

### 17c — Preflight readiness

- **D-13 (check split): generic core + optional adapter hook.** A generic
  preflight runs the universal checks; `AgentAdapter` gains a `preflight()`
  method with an empty default body, mirroring the existing `extra_env`
  default (`agents/mod.rs:39-41`). Adapters opt in only where they differ.
  This is the trait surface Phase 18's Hermes adapter implements — it must
  consume this model, not define a competing one.

- **D-14 (universal vs adapter):** RESOLVES retrospective decision-gate Q3.
  - **Universal (generic layer):** plan interactivity vs. execution mode;
    required security artifact present; external credential validity.
  - **Adapter hook:** reviewer receiver set non-empty.

- **D-15 (failure semantics): named preflight gate + notify.** Not a hard exit.
  Unattended runs are the design target — the notify hook exists precisely
  because the operator is not watching the terminal, so a hard exit to stdout
  is invisible to a cron-launched run. Consistent with the WR-11 never-silent
  idiom.

- **D-16 (timing):** before every stage launch, scoped to that stage's
  requirements. Directly fixes the observed miss — Ship's empty reviewer set
  and invalid GitHub auth surfaced only after Ship's work had run. A single
  up-front check cannot evaluate Ship-specific requirements hours ahead, nor
  catch credentials that expire mid-phase.

### 17d — Build provenance

- **D-17 (self-dogfood detection): workspace identity match.** The target
  project root contains the DevFlow workspace (a `Cargo.toml` declaring
  `devflow-cli`/`devflow-core`). Deterministic, offline, no config, no false
  positives on unrelated Rust projects. Rejected: git-remote match (breaks on
  forks, SSH-vs-HTTPS spellings, remote-less clones) and explicit opt-in
  (failure mode is forgetting to set it — which is how the incident happened).

- **D-18 (strictness): block self-dogfood on stale, warn elsewhere.** Strict is
  the default where it is cheap. Only affects DevFlow's own repo; ordinary
  projects running a released binary are untouched and need no source
  checkout. Justified by cost: the incident consumed an entire spike phase
  reading false evidence.

- **D-19 (staleness definition): composite.** Stale = embedded commit is not an
  ancestor of HEAD, **OR** source is newer than the build timestamp. Ancestry
  catches the exact incident (a Homebrew symlink to a release build predating
  the phase's fixes); the mtime arm catches an uncommitted working tree the
  binary predates, which ancestry alone misses. Rejected: bare `commit != HEAD`
  (fires on any normal in-development state — alarms you learn to ignore are
  worse than none) and mtime-only (fragile across checkout/clone/rebase).

- **D-20 (build metadata): hand-rolled `build.rs`, no new dependencies.** ~30
  lines shelling to git, emitting `cargo:rustc-env` vars. No `build.rs` or
  version-embedding infrastructure exists today. Rejected `vergen` — a build
  dependency and its tree for what a few readable lines do. MUST degrade
  gracefully when git metadata is unavailable (crates.io installs have no
  `.git`) — absence of provenance is not staleness.

- **D-21 (event payload):** `workflow_started` currently carries only
  agent/mode/worktree (`main.rs:605-614`). Extend with version, commit, dirty
  flag, build timestamp, and resolved executable path. `std::env::current_exe()`
  is already used at `monitor.rs:79` — reuse that precedent.

### Claude's Discretion

- Exact typed-outcome variant names and the shape of the structured evidence
  record (D-10), within the schema-v1 convention.
- Separate-counter ceiling values and backoff curve for D-08/D-09.
- Whether the D-12 pure policy function lives in a new `devflow-core` module or
  an existing one.
- Preflight check implementation order and how stage-scoped requirements are
  declared (D-16).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and evidence
- `.planning/phases/17-pipeline-dogfood-followup/17-DOGFOOD-RETROSPECTIVE.md`
  — the scoping doc: confirmed findings, candidate capabilities, decision gate,
  and the five initial acceptance criteria. Remains the authority on WHAT each
  item is; decisions above layer on top. Note its decision-gate Q2 and Q4 are
  answered in this file, and Q3 is answered by D-14.
- `.planning/phases/17-pipeline-dogfood-followup/CONTEXT.md` — thin status
  pointer (priority, depends-on, blocks Phase 18).

### Code this phase changes
- `crates/devflow-core/src/agent_result.rs` — `AgentStatus` enum (lines 41-50);
  `evaluate_layer0` (627-690, the D-05 target); `evaluate_layer3` (592-624, the
  D-02 target); the four-layer cascade in `evaluate_agent_result_inner`
  (702-725).
- `crates/devflow-cli/src/main.rs` — `advance()` (788-887), the D-01 defect at
  854/871; `handle_validate_outcome` (891), `handle_ship_outcome` (928),
  `handle_stage_failure` (981), `transition` (1127); `workflow_started` emit
  (605-614, D-21); agent-binary preflight (673-690, the only preflight today).
- `crates/devflow-core/src/agents/mod.rs` — `AgentAdapter` trait (line 11);
  `extra_env`'s default impl (39-41) is the precedent D-13 follows.
- `crates/devflow-core/src/events.rs` — schema v1; D-10 extends this, does not
  replace it.
- `crates/devflow-core/src/verify.rs` — Layer 0's command source and the
  `TRUST_EXTERNAL_VERIFY_ENV` approval mechanism that D-05 must preserve.
- `crates/devflow-core/src/config.rs` — `devflow.toml` loader and
  `external_verify_enabled` knob (lines 58-162).

### Prior decisions that bind this phase
- `.planning/phases/16-pipeline-reliability-hardening/16-CONTEXT.md` — D-03
  (`devflow.toml` exists; precedence env > file > default), D-05 (checkers as
  cargo tests, no new hook machinery in a hardening phase).
- `.planning/phases/16-pipeline-reliability-hardening/16-VERIFICATION.md` §16a
  — what Layer 0 was verified to deliver ("approved PLAN-only Layer 0 outranks
  self-report and fails closed").
- `.planning/phases/16-pipeline-reliability-hardening/16-01-SUMMARY.md:132` —
  "16a should treat an effect-specific external post-condition as
  higher-confidence than generic hook/process success" — directly supports D-05.
- `.planning/phases/16-pipeline-reliability-hardening/16-REVIEW.md` §WR-03 —
  the deferred test race, now Phase 18's 18e. Do NOT fix it here.
- `.planning/STATE.md` — Decisions log, 2026-07-18 entry: Phase 17 scoping and
  the P5/P6 deferral to Phase 18.
- `.planning/codebase/CONCERNS.md` — useful for context, but see Deferred: its
  main.rs line count is stale and its framing oversells that problem.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Layer 0 external verification** (`agent_result.rs:627-690` + `verify.rs`) —
  the mechanism D-03 needs already exists; 17a extends it rather than building
  a parallel one.
- **`cron-instructions-NN.json`** rate-limit resume machinery — D-09's
  auto-resume path extends this.
- **`events.jsonl` schema v1 + `devflow logs`** — D-10's structured evidence
  derives from this.
- **`extra_env` default-impl idiom** (`agents/mod.rs:39-41`) — the exact
  pattern for D-13's `preflight()`.
- **`std::env::current_exe()`** (`monitor.rs:79`) — precedent for D-21's
  executable-path resolution.
- **`prepare_loop_back_to_code` split** (13-01) — the project's established
  precedent for testability-driven extraction; D-12 follows it.

### Established Patterns
- Library + thin CLI: logic in `devflow-core`, `main.rs` stays a wrapper. D-12
  respects this; the full-extraction alternative does not (see Deferred).
- No panics in library code; `thiserror` typed errors throughout.
- Stage prompts are static strings shared across adapters — anything D-13 adds
  must stay harness-agnostic.
- Minimal live tests: verification probes should be the cheapest workload that
  crosses the real seam. Applies to 17c preflight probes and 17d's staleness
  check — both run on every stage launch and must never be worth skipping.

### Integration Points
- `evaluate_agent_result_inner`'s four-layer cascade — D-02 and D-05 both
  change layer semantics; trace ALL layers for ALL stage types before editing
  (`CONCERNS.md` flags this cascade as a fragile area with under-tested Layer 3).
- `advance()`'s failure/success dispatch — D-02's typed outcomes land here.
- Stage launch path in `launch_stage` (`main.rs:692`) — D-16's per-stage
  preflight hooks in ahead of this.
- `.devflow/events.jsonl` writers — D-10 and D-21 both extend payloads.

</code_context>

<specifics>
## Specific Ideas

- **External checks as a first-class validation step** (operator's framing,
  2026-07-18): the originating incident was a stage doing legitimate external
  work that produced no commits. The operator's concern is not only that
  `Unknown` must stop auto-advancing, but that a stage SHOULD be able to
  declare "my deliverable is external" and have a passing probe count as real
  completion evidence. D-05 is that, and it is the load-bearing half of 17a —
  without it, D-03 punishes legitimate external-only stages.

- **The three-way distinction matters more than the fail-closed rule itself.**
  "No code produced" and "nothing accounted for what was done" are different
  states. Only the second is a failure.

</specifics>

<deferred>
## Deferred Ideas

- **Full `main.rs` orchestration extraction** — `CONCERNS.md` recommends
  pulling `advance`/`transition`/`handle_*_outcome` into a separate module.
  Deliberately NOT done in Phase 17, decided 2026-07-18 after direct
  examination:
  1. The stated premise is stale — CONCERNS.md says 3,334 lines; the file is
     3,806 with `#[cfg(test)]` starting at line 2643, so production code is
     ~2,640 lines and the target cluster (788–1330) is ~540 contiguous lines of
     small functions (`advance` ~100, `handle_validate_outcome` ~37,
     `transition` ~26, `finish_workflow` ~38).
  2. **It is not a file move — it is an error-type redesign.** Every function
     in the cluster returns `Result<(), CliError>`, and `CliError` is a
     CLI-crate presentation type. Extracting to `devflow-core` requires either
     dragging `CliError` down into core (wrong direction) or introducing a core
     error type plus conversions at every call site.
  3. Risk asymmetry: that diff runs through exactly the fail-closed paths
     Phase 17 exists to prove correct. Verification confidence depends on a
     small diff where every changed line traces to a named defect; a Phase 17
     verification failure would have two candidate causes.
  4. The project's own precedent (13-01) is narrower and motivated by
     testability — which D-12 follows.

  If the cluster still warrants extraction after 17 ships, it earns its own
  phase where a regression is attributable because nothing else moved.

- **Correct `CONCERNS.md`'s main.rs line count and framing** — it reports 3,334
  lines (stale, and counts tests). Small doc-accuracy fix; note that Phase 16's
  16c doc-claim checker scans operator-facing docs only and does NOT cover
  `.planning/`, so this class of drift is unchecked by design.

- **18d — project-aware `devflow doctor` reconciliation** — moved to Phase 18
  on 2026-07-18. `doctor()` takes `_project_root` unused (`main.rs:2454`) and
  only checks external tools/PATH. Depends on D-10's evidence records and
  D-21's provenance.

- **18e — WR-03 test stabilization** — moved to Phase 18 on 2026-07-18.
  `parallel_creates_two_worktrees_and_spawns_two_monitors`
  (`crates/devflow-cli/tests/phase7_cli.rs:184-200`) races the monitor's
  capture archival. Distinct from `13-REVIEW.md`'s WR-03 in `git.rs:286` — the
  per-phase review numbering namespaces collide.

</deferred>

---

*Phase: 17-pipeline-dogfood-followup*
*Context gathered: 2026-07-18*
