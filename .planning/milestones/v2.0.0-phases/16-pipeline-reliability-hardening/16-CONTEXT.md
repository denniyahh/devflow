# Phase 16: Pipeline Reliability Hardening - Context

**Gathered:** 2026-07-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Make the pipeline's own completion/review signals trustworthy, and make
Ship's gating review catch what it currently misses on a single pass. Nine
scope items (16a–16i), each traced to a specific observed incident in the
Phase 15 dogfood run — full evidence and per-item scope in the scoping doc
(`CONTEXT.md` in this directory, MUST-read).

Explicitly out of scope: Hermes support (Phase 17), Antigravity adapter
(unscheduled backlog). Depends on Phase 15 actually shipping first.

</domain>

<decisions>
## Implementation Decisions

### 16d/16e — Ship review pipeline
- **D-01 (locus): Adaptive hybrid, capability-conditional.** One shared Ship
  prompt (in `prompt.rs`) carries the angle list plus a conditional
  instruction: run the angles as parallel finder subagents where the harness
  supports them (Claude Code), otherwise as sequential focused single-angle
  passes — all merging findings into one `REVIEW.md`. No new DevFlow process
  management. Rationale: subagent support is NOT universal (Codex CLI has no
  first-class primitive; OpenCode partial); sequential narrow passes preserve
  recall better than one broad generalist pass, which is the failure mode
  that caused four Ship loop-backs.
- **D-02 (angles): Config-extensible list.** Built-in defaults are the four
  incident-derived angles — doc-accuracy cross-reference, security/leaked-
  data, CI/build correctness, external-state claims — plus one generalist
  deep pass. Overridable via config (see D-03). Angle list snapshot-tested
  in `prompt.rs`.
- **D-03 (config): The no-config-file decision is REOPENED — deliberately.**
  Phase 16 introduces a minimal `devflow.toml` (TOML), carrying ONLY Phase
  16's new knobs (review angles, capture-retention N, verification
  settings). Existing `DEVFLOW_*` env vars are untouched. Precedence:
  env var > file > built-in default. This reverses the Phase 11/15a
  no-config-file stance with full knowledge of it; the `config.rs` doc
  comment claiming "no config file" must be updated. Env-var migration and
  full pipeline configurability remain out of scope (deferred).
- **D-04 (16e gating): Planner's discretion.** Per-wave incremental review
  depth and whether findings block wave advancement were not discussed —
  planner decides within D-01..D-03.

### 16c/16i — Deterministic checkers
- **D-05 (enforcement point): Cargo tests in the workspace.** Both checkers
  are `#[test]` functions — they run locally, in CI, and inside the Code
  stage via the agent's own test run (an agent cannot self-report success
  past a failing invariant test). No new hook machinery in a hardening
  phase. If dogfooding later shows agents skipping the test suite, promoting
  the checkers to a Code-stage hook is a small deferred follow-up.
- **D-06 (16c scope): Existence + pinned claims.** Generic layer: every env
  var, CLI flag/subcommand, `DEVFLOW_*` knob, and file path named in
  operator docs must exist in source. Plus a hand-maintained set of pinned
  assertions for specific value/behavior claims (e.g. the documented
  `RUST_LOG` default must match actual `EnvFilter` behavior). NOTE:
  existence-only checking would NOT have caught the RUST_LOG default-value
  incident that motivated 16c — the pinned-claims layer is what covers
  semantic doc claims. Generic value-extraction from prose was rejected
  (false positives erode trust in the checker).
- **D-07 (false positives): Scoped scan + allowlist file.** Scan only
  operator-facing docs (README, ARCHITECTURE, CONTRIBUTING, OPERATIONS,
  docs/guides); skip CHANGELOG and `.planning/` entirely. A checked-in
  allowlist file holds known exceptions, each entry with a required reason
  comment, so exceptions are visible and reviewed in diffs.
- **D-08 (direction): Bidirectional.** Docs→source: everything named in
  docs exists. Source→docs/gitignore: every `DEVFLOW_*` env var and CLI
  subcommand is documented, and every `.devflow/`-writing path in source is
  covered by `.gitignore` (16i is exactly this direction).

### Sequencing
- **D-09: 16k first.** User-confirmed 2026-07-17: begin the phase with the
  16k forensics (gate-approval advance path, VersionBump wrong-checkout
  bug, unconditional hook success) — every other reliability item assumes
  the terminal pipeline signal means something, and 16k's findings will
  inform 16a's verification design and 16f's root resolution.

### Claude's Discretion
- 16e per-wave review gating semantics and depth (D-04).
- 16a verification-contract syntax/timing/failure semantics, 16b/16h capture
  retention layout and history surfacing, 16f/16g fixes — not discussed;
  planner works from the scoping doc's per-item statements. 16b's
  retention-N and 16a's settings should land as `devflow.toml` knobs per
  D-03 where a knob is warranted.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase scope and evidence
- `.planning/phases/16-pipeline-reliability-hardening/CONTEXT.md` — the
  scoping doc: goal, dogfood-run evidence, and per-item scope for 16a–16i.
  The decisions above layer on top of it; it remains the authority on WHAT
  each item is.

### Code this phase changes or constrains
- `crates/devflow-core/src/prompt.rs` — Ship prompt currently hardcodes a
  single-pass `/gsd-code-review {phase}`; D-01/D-02 replace this. Existing
  snapshot tests show the expected test idiom.
- `crates/devflow-core/src/config.rs` — the "no config file" doc comment
  being deliberately reversed by D-03; git-flow constants live here.
- `crates/devflow-core/src/agent_result.rs` — capture-path helpers
  (`stdout_path`/`stderr_path`/`exit_code_path`) and the cleanup that wipes
  them per launch (16b's target); also the Layer-2 commit-count fallback
  implicated in the first false positive.
- `crates/devflow-core/src/events.rs` / `.devflow/events.jsonl` (schema v1)
  — 16h should derive from this, not invent a new store.

### Prior decisions that bind this phase
- `.planning/STATE.md` — Decisions log: Phase 16 insertion rationale
  (2026-07-17), Phase 14 multi-angle post-ship review precedent
  (2026-07-16), no-config reaffirmation via `.devflow.yaml` decoy removal.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `prompt.rs` snapshot-test pattern (`ship_prompt_sequences_code_review_before_ship`)
  — extend for the angle list and conditional fan-out instruction.
- `events.jsonl` schema v1 + `devflow logs` — foundation for 16h.
- Existing `DEVFLOW_*` env-var idiom — the override layer above `devflow.toml`.

### Established Patterns
- Library + thin CLI: all logic in `devflow-core`, `main.rs` stays a wrapper.
- No panics in library code; `thiserror` typed errors throughout.
- Stage prompts are static strings shared across all agent adapters —
  D-01's conditional instruction must stay harness-agnostic.

### Integration Points
- Ship stage prompt composition (`stage_prompt(Stage::Ship, phase)`).
- Code stage's implicit `cargo test` run — where D-05's checkers bite.
- `.devflow/` write paths enumerated across `agent_result.rs`, `lock.rs`,
  `ship.rs`, `events.rs`, `gates.rs` — the 16i test must enumerate these
  from source, not from a hardcoded list.

</code_context>

<specifics>
## Specific Ideas

- The Phase 14 post-ship review (8 finder angles, 1-vote verification,
  documented in `14-REVIEW.md`) is the explicit quality bar for what 16d's
  deep review should catch in one pass.
- Operator's standing principle (minimal live tests): verification probes
  should be the cheapest workload that crosses the real seam — applies to
  16a probe design and to keeping the 16c/16i tests fast enough to never be
  worth skipping.

</specifics>

<deferred>
## Deferred Ideas

- **[PROMOTED to scope as 16j, 2026-07-17 — see scoping doc CONTEXT.md]**
  Original candidate note (16j?) — operator notification gap, observed
  2026-07-17:** the Phase 15 ship security gate fired (`gate_fired` +
  `notify_fired` both logged to events.jsonl, `unexpected: false`) yet the
  operator received NO actual notice and only discovered the ~78-minute-old
  blocked gate by manually asking for status. The notify path "fired"
  according to DevFlow's own telemetry while delivering nothing a human saw
  — a pipeline-feedback false positive, same family as the Code-stage false
  positives that motivated this phase. Needs: a notify channel that
  verifiably reaches the operator (and/or a loud persistent indicator in
  `devflow status`/terminal), not just an event log entry claiming success.
  User to promote via /gsd-phase if wanted in Phase 16 proper.
- **16f scope broadening — root resolution affects ALL subcommands, observed
  2026-07-17:** `devflow gate approve 15` failed with "no open gate for
  phase 15" when run from inside the phase worktree — same root cause as
  16f's `status` symptom: `project` defaults to `.` and DevFlow never walks
  up to find the real `.devflow/` in the primary checkout. USER-CONFIRMED
  for Phase 16: fix as a shared walk-up resolver used by every subcommand
  (status, gate, logs, recover, …), not a per-command patch. The live gate
  approval had to be re-run from the main checkout to land.
- **Gate CLI positional-arg footgun, observed 2026-07-17:** `devflow gate
  approve 15 ship` fails with `error: project path does not exist: ship` —
  the trailing positional project-root argument silently swallows a
  misplaced `--stage` value and produces an error with no hint toward the
  actual mistake. USER-CONFIRMED for Phase 16 (fits the 16g UX-cleanup
  bucket): either make `stage` positional-optional, drop the positional
  project in favor of `--project`, or add a "did you mean `--stage ship`?"
  hint when the unmatched positional equals a stage name.
- **[PROMOTED to scope as 16k, 2026-07-17 — see scoping doc CONTEXT.md]**
  Original candidate note (16k?) — ship terminal false positive, observed
  2026-07-17 (SEVERE, the worst signal failure of the entire dogfood run):**
  after the operator approved the final "Ship complete — approve merge?"
  gate, DevFlow emitted `hook_run VersionBump ok=true`, `hook_run
  BranchCleanup ok=true`, and `workflow_finished` — while in reality: (1)
  NO merge was performed, locally or via the PR the ship agent had opened
  (PR #7 was still open, feature/phase-15 not an ancestor of develop); (2)
  VersionBump mutated the PRIMARY checkout — which contained none of the
  phase's work — leaving an uncommitted Cargo.toml bump to a nonsense
  version (1.2.0 → 1.2.183) and tagging v1.2.183 on an unrelated docs
  commit; (3) BranchCleanup deleted nothing (worktree and branch intact)
  yet reported ok=true; (4) per-phase state was deleted, so `devflow
  status` showed nothing wrong. The phase had to be shipped by hand (manual
  merge 01d511c, v1.3.0). Forensic targets: the gate-approval advance path
  in ship.rs/main.rs — where the merge step should be and isn't (or fails
  silently), why VersionBump computes x.y.<commit-count?> against the wrong
  checkout, and why hook success is unconditional. Related earlier evidence:
  CHANGELOG.md contains a series of bogus auto-entries (1.2.175/176/179
  "Released phase via DevFlow") from the same mechanism.

- Full pipeline configurability via `devflow.toml` (stage behavior, hooks,
  agent defaults) — shelved 2026-07-08, still deferred; only the minimal
  new-knob file is in Phase 16.
- Migrating existing `DEVFLOW_*` env vars into `devflow.toml` — future
  phase, only if the minimal file proves itself.
- Promoting the 16c/16i cargo tests to a Code-stage DevFlow hook — only if
  dogfooding shows agents skipping the test suite.

</deferred>

---

*Phase: 16-pipeline-reliability-hardening*
*Context gathered: 2026-07-17*
