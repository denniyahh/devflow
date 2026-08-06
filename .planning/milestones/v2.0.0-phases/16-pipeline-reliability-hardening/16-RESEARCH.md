# Phase 16: Pipeline Reliability Hardening - Research

**Researched:** 2026-07-17
**Domain:** Internal CLI/pipeline-orchestration reliability (Rust) — completion-signal
trust, review depth, deterministic doc/gitignore invariants, CLI ergonomics, git-flow
correctness
**Confidence:** HIGH for architecture/pitfalls (all traced to read source + reproduced
incidents); MEDIUM for external-library choices (websearch-verified, no first-party docs
fetch available in this session); LOW/ASSUMED flagged individually below.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**16d/16e — Ship review pipeline**
- **D-01 (locus): Adaptive hybrid, capability-conditional.** One shared Ship prompt (in
  `prompt.rs`) carries the angle list plus a conditional instruction: run the angles as
  parallel finder subagents where the harness supports them (Claude Code), otherwise as
  sequential focused single-angle passes — all merging findings into one `REVIEW.md`. No
  new DevFlow process management. Rationale: subagent support is NOT universal (Codex CLI
  has no first-class primitive; OpenCode partial); sequential narrow passes preserve
  recall better than one broad generalist pass, which is the failure mode that caused four
  Ship loop-backs.
- **D-02 (angles): Config-extensible list.** Built-in defaults are the four
  incident-derived angles — doc-accuracy cross-reference, security/leaked-data, CI/build
  correctness, external-state claims — plus one generalist deep pass. Overridable via
  config (see D-03). Angle list snapshot-tested in `prompt.rs`.
- **D-03 (config): The no-config-file decision is REOPENED — deliberately.** Phase 16
  introduces a minimal `devflow.toml` (TOML), carrying ONLY Phase 16's new knobs (review
  angles, capture-retention N, verification settings). Existing `DEVFLOW_*` env vars are
  untouched. Precedence: env var > file > built-in default. This reverses the Phase 11/15a
  no-config-file stance with full knowledge of it; the `config.rs` doc comment claiming
  "no config file" must be updated. Env-var migration and full pipeline configurability
  remain out of scope (deferred).
- **D-04 (16e gating): Planner's discretion.** Per-wave incremental review depth and
  whether findings block wave advancement were not discussed — planner decides within
  D-01..D-03.

**16c/16i — Deterministic checkers**
- **D-05 (enforcement point): Cargo tests in the workspace.** Both checkers are `#[test]`
  functions — they run locally, in CI, and inside the Code stage via the agent's own test
  run (an agent cannot self-report success past a failing invariant test). No new hook
  machinery in a hardening phase. If dogfooding later shows agents skipping the test
  suite, promoting the checkers to a Code-stage hook is a small deferred follow-up.
- **D-06 (16c scope): Existence + pinned claims.** Generic layer: every env var, CLI
  flag/subcommand, `DEVFLOW_*` knob, and file path named in operator docs must exist in
  source. Plus a hand-maintained set of pinned assertions for specific value/behavior
  claims (e.g. the documented `RUST_LOG` default must match actual `EnvFilter` behavior).
  NOTE: existence-only checking would NOT have caught the RUST_LOG default-value incident
  that motivated 16c — the pinned-claims layer is what covers semantic doc claims. Generic
  value-extraction from prose was rejected (false positives erode trust in the checker).
- **D-07 (false positives): Scoped scan + allowlist file.** Scan only operator-facing docs
  (README, ARCHITECTURE, CONTRIBUTING, OPERATIONS, docs/guides); skip CHANGELOG and
  `.planning/` entirely. A checked-in allowlist file holds known exceptions, each entry
  with a required reason comment, so exceptions are visible and reviewed in diffs.
- **D-08 (direction): Bidirectional.** Docs→source: everything named in docs exists.
  Source→docs/gitignore: every `DEVFLOW_*` env var and CLI subcommand is documented, and
  every `.devflow/`-writing path in source is covered by `.gitignore` (16i is exactly this
  direction).

**Sequencing**
- **D-09: 16k first.** Begin the phase with the 16k forensics (gate-approval advance path,
  VersionBump wrong-checkout bug, unconditional hook success) — every other reliability
  item assumes the terminal pipeline signal means something, and 16k's findings inform
  16a's verification design and 16f's root resolution.

### Claude's Discretion
- 16e per-wave review gating semantics and depth (D-04).
- 16a verification-contract syntax/timing/failure semantics, 16b/16h capture retention
  layout and history surfacing, 16f/16g fixes — not discussed; planner works from the
  scoping doc's per-item statements. 16b's retention-N and 16a's settings should land as
  `devflow.toml` knobs per D-03 where a knob is warranted.

### Deferred Ideas (OUT OF SCOPE)
- Full pipeline configurability via `devflow.toml` (stage behavior, hooks, agent
  defaults) — shelved 2026-07-08, still deferred; only the minimal new-knob file is in
  Phase 16.
- Migrating existing `DEVFLOW_*` env vars into `devflow.toml` — future phase, only if the
  minimal file proves itself.
- Promoting the 16c/16i cargo tests to a Code-stage DevFlow hook — only if dogfooding
  shows agents skipping the test suite.
- Hermes support (Phase 17), Antigravity adapter (unscheduled backlog).

**Note on the scoping doc's own `<deferred>` block:** two further items are textually
inside `16-CONTEXT.md`'s `<deferred>` section but are marked "USER-CONFIRMED for Phase
16" in their own text and are treated here as IN SCOPE (the scoping doc `CONTEXT.md`
folds them into 16f and 16g respectively):
- **16f is broadened**: the walk-up resolver must be shared by every subcommand (status,
  gate, logs, recover, …), not just `status` — `devflow gate approve 15` fails identically
  to `status` when run from inside the phase worktree.
- **16g gains a second item**: the `gate approve`/`reject` positional-arg footgun
  (`devflow gate approve 15 ship` silently treats `ship` as the trailing `project`
  positional and fails with an unhelpful "project path does not exist: ship").
</user_constraints>

<phase_requirements>
## Phase Requirements

No `.planning/REQUIREMENTS.md` exists in this project (confirmed: file absent). This
phase's requirement IDs are the scope items `16a`–`16k` defined in
`.planning/phases/16-pipeline-reliability-hardening/CONTEXT.md` (the scoping doc), each
tied to a specific, reproduced incident from the Phase 15 dogfood run. Per D-09, 16k is
sequenced first.

| ID | Description | Research Support |
|----|-------------|------------------|
| 16k | Ship terminal false positive — gate-approval advance path forensics (sequenced FIRST per D-09) | Root cause fully located: `hooks_after_ship()` (`hooks.rs:84-86`) returns only `[VersionBump, BranchCleanup]` — **no merge hook is wired into the terminal path at all**, even though `GitFlow::feature_finish` (`git.rs:71`, merges + deletes) already exists and is exercised only by tests. `VersionBump`'s `compute_version` (`version.rs:142`) runs `git tag`/`git rev-list --count HEAD` against whatever `project_root` was passed to `finish_workflow` — the PRIMARY checkout in a worktree-based run — explaining the "wrong checkout" tag. `run_checkout_hooks` (`main.rs:929`) reports `hook_run ok: outcome.is_ok()` per hook — `BranchCleanup`'s `Ok(())` on a no-op non-merged branch is a *correctly* fail-soft outcome per its own contract, not a bug in that hook; the real defect is the missing merge step upstream, which never gives `BranchCleanup` a merged branch to react to. |
| 16a | External post-condition verification for plans with no repo-diff success signal | `agent_result::evaluate_layer1` (`agent_result.rs:465`) treats the agent's own `DEVFLOW_RESULT: success` as authoritative whenever `is_error` isn't set — Layer 2's commit-count fallback (`agent_result.rs:494`) is the only other check, and both were fooled on the crates.io-publish plan. A new "Layer 0" external verification command, run by DevFlow itself and independent of agent self-report, closes this; crates.io's sparse index (`index.crates.io/<name>`) is a concrete, rate-limit-free probe target. |
| 16b | Retained per-stage capture history | `agent_result::cleanup_phase_files` (`agent_result.rs:673`) is called unconditionally from `launch_stage` (`main.rs:634`) on **every** stage transition, wiping `.devflow/phase-NN-{stdout,exit}` for the stage that just finished before a human can inspect it if the automated evaluation was wrong. |
| 16c | Deterministic doc-claim checker | No existing tool in the codebase does this; nearest precedent is `--help` snapshot testing (Phase 15a) and the `prompt.rs` snapshot-test idiom to model after structurally. |
| 16d | Ship review: deep mode + multi-angle parallel review | `prompt.rs::ship_stage_prompt` (`prompt.rs:52`) currently sequences exactly one `/gsd-code-review {N}` (standard depth, single generalist pass) before the Critical-severity gate. The existing `14-REVIEW.md` (8 finder angles: 3×correctness, reuse, simplification, efficiency, altitude, conventions → dedup → 1-vote verify) is the proven multi-angle pattern already used once in this project — 16d's angle *categories* are incident-derived (doc-accuracy, security/leaked-data, CI/build correctness, external-state claims) and differ from 14's code-quality categories, but the fan-out/dedup/merge mechanics are the same shape. |
| 16e | Incremental per-plan/per-wave review | No wave-boundary review hook currently exists; `run_checkout_hooks`/`hooks_for_transition` (`hooks.rs:76`) is the only per-transition hook-firing mechanism to extend. |
| 16f | Worktree-aware `devflow status` (+ broadened to a shared walk-up resolver for every subcommand) | `fn project_root(project: PathBuf)` (`main.rs:2093`) only canonicalizes the given path (default `.`) — zero walk-up logic. Every subcommand (`status`, `gate approve/reject`, `logs`, `recover`, …) calls this same function, so a single shared resolver fix (loop `Path::parent()` looking for `.devflow/`) fixes all call sites at once. |
| 16g | Legacy-state WARN cleanup (+ gate CLI positional-arg footgun) | `workflow::migrate_legacy_state` (`workflow.rs:50-61`) fires `warn!("legacy state at {} is unparsable — leaving it in place", …)` on every `list_states`/`load_state` call (i.e. every `status`) with no mention of `devflow recover --clean`. Separately, `GateCmd::Approve`/`Reject` (`main.rs:234-264`) each declare TWO bare positionals (`phase: u32`, then `project: PathBuf` after several `--flag`s) — clap's documented trailing-positional-plus-subcommand footgun (verified via websearch) explains `devflow gate approve 15 ship` silently binding `"ship"` to `project` and failing as "project path does not exist: ship". |
| 16h | Cross-attempt Ship/Code history view | `events.jsonl` (`events.rs`, schema v1, `last_events_by_phase`) is the only existing structured history; `REVIEW.md` files are unstructured markdown with no cross-attempt diffing today. 16h should read `events.jsonl` (not invent a new store) and correlate against retained `REVIEW.md`/capture-history artifacts from 16b. |
| 16i | `.gitignore`/runtime-file CI invariant | Confirmed live: enumerated every `.devflow`-path constructor across `events.rs`, `workflow.rs`, `lock.rs`, `gates.rs`, `agent_result.rs`, `ship.rs`, `monitor.rs`; the current `.gitignore` (repo root) DOES cover all of them as of 2026-07-17 (the incomplete-fix gap from the dogfood run was closed manually during Phase 15). 16i's job is to make this an automated, regression-proof invariant, not to fix a currently-broken state. |
| 16j | Verifiable operator notification | `gates::fire_gate_notify`/`run_notify_command` (`gates.rs:275-317`) is fire-and-forget: it only checks the notify command's own exit status (`debug!`/`warn!`), which is exactly the same class of false-positive as the Code-stage incidents — "the mechanism reported success" without knowing whether a human ever saw it. Websearch confirms ntfy/desktop-notify give no delivery-receipt API; the only genuinely verifiable signal is a persistent, still-visible indicator that survives until the operator acts (the gate response itself, or a loud `devflow status` banner), not the notify command's exit code. |

</phase_requirements>

## Summary

Phase 16 is not a "pick a new stack" phase — it hardens an existing, working ~11,000-line
Rust CLI (`devflow-core` + `devflow-cli`) whose failure modes were all *observed*, not
hypothetical, during the Phase 15 dogfood run. Research for this phase is almost entirely
codebase archaeology: for every scope item (16a–16k) the exact function, line range, and
causal mechanism of the incident it targets has been located and is cited below. The one
genuinely new piece of infrastructure is `devflow.toml` (D-03), for which the `toml` crate
(serde-compatible, 10+ years old, 13M+ weekly downloads) is the obvious, uncontested
choice — verified to exist and be healthy via the package-legitimacy registry check, but
its exact version/API surface is `[ASSUMED]` from training knowledge + websearch, not
fetched from first-party docs in this session (no Context7/docs MCP was available; flagged
in the Assumptions Log).

The single most consequential finding is **16k's root cause**: the terminal "Ship complete
→ approve merge?" gate-approval path (`finish_workflow` in `main.rs:1054`) never calls
`GitFlow::feature_finish` (the existing merge+delete function in `git.rs:71`) at all — it
only runs `VersionBump` and `BranchCleanup`, neither of which merges anything. This is not
a subtle race or a partial regression; the merge step is simply absent from
`hooks_after_ship()`. Per D-09 this must be fixed (or its blast radius fully understood)
before 16a/16f can be designed with confidence, because both assume the terminal signal
("workflow_finished") means what it claims.

**Primary recommendation:** Fix 16k first by wiring `GitFlow::feature_finish` (or an
equivalent explicit merge hook) into the pre-`VersionBump` terminal path, sequenced so
version/tag computation runs against the post-merge `develop` state, not the worktree's
private branch tip — then build 16a's external-verification "Layer 0" and 16f's shared
walk-up resolver on top of a terminal signal that has actually been fixed to mean what it
says.

## Architectural Responsibility Map

DevFlow is a single-process CLI + library (no client/server split); "tiers" here are the
internal architectural layers already established by the codebase's own module
boundaries.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Stage sequencing / gate decisions (16k, 16e) | CLI orchestrator (`devflow-cli/main.rs`) | Core library (`workflow.rs`, `mode.rs`) | `advance()`/`transition()`/`run_gate()` live in `main.rs`; pure state-transition logic is factored into `devflow-core` for unit-testability |
| Completion-signal evaluation (16a, agent self-report) | Core library (`agent_result.rs`) | External agent process | Parsing/Layer-1/Layer-2 logic is pure and lives in the library; the untrusted input is the agent CLI's own stdout |
| Capture/history retention (16b, 16h) | Core library (`agent_result.rs`, `events.rs`) | Filesystem (`.devflow/`) | Path construction + cleanup are library functions; the actual store is flat files under `.devflow/` |
| Ship review depth/angles (16d, 16e) | Prompt composition (`prompt.rs`) | External agent process | DevFlow only composes and hands off the prompt text; the agent (harness-dependent) executes the review |
| Deterministic checkers (16c, 16i) | Core library test suite (`#[test]` fns per D-05) | CI (`cargo test` in `.github/workflows/ci.yml`) | D-05 locks these as library-level tests, not a new hook subsystem |
| Project-root resolution (16f) | CLI arg parsing (`main.rs::project_root`) | Filesystem walk-up | Currently a pure canonicalize with no walk-up; the fix is a shared library-level resolver called from every `Command` arm |
| CLI ergonomics (16g footgun) | CLI arg parsing (`clap` `Subcommand`/`Arg` definitions) | — | Pure clap schema-design fix, no runtime logic involved |
| Notification delivery (16j) | Core library (`gates.rs::fire_gate_notify`) | External notify command / `devflow status` | The notify command is an opaque external process; DevFlow's own `status` output is the only tier DevFlow fully controls, hence the recommended fallback of a persistent status indicator |
| Git-flow merge/version/tag (16k) | Core library (`git.rs`, `version.rs`, `hooks.rs`) | Primary checkout (external state) | Merge/tag are pure git operations already implemented in `git.rs`; the bug is a *wiring* gap in `hooks.rs`/`main.rs`, not a missing capability |
| Config precedence env > file > default (D-03) | Core library (`config.rs`) | Filesystem (`devflow.toml`) | New `GitFlowConfig`-adjacent struct/loader in `config.rs`, following the existing `Default`-only-constructor idiom |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `toml` [ASSUMED→verified — see Assumptions Log A1] | v1.1.3 (measured 2026-07-17 via `cargo add toml -p devflow-core --dry-run`; supersedes the earlier ~0.9.x websearch figure). Plan pins `toml = "1"` (semver-compatible) | Parse/serialize the new minimal `devflow.toml` (D-03), serde-compatible | 10+ years old (first published 2014), 13.4M weekly downloads, canonical `toml-rs/toml` repo — the only serious choice in the Rust ecosystem for this |
| `serde`/`serde_json` (already a workspace dep) | `1` (workspace pin, unchanged) | Existing envelope/state/event (de)serialization; `devflow.toml`'s Rust struct also derives `Deserialize` | Already used everywhere in this codebase — no new pattern needed |
| `clap` (already a workspace dep) | `4` (workspace pin, unchanged) | CLI arg parsing — 16g's footgun fix is a *schema* change within the existing `clap` derive macros, not a new dependency | Already the CLI framework |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `ignore` (BurntSushi/ripgrep's gitignore-matching crate) [ASSUMED — not currently a dependency] | latest (2.5M+ weekly downloads, since 2016, verified via registry check) | Only if 16i's implementation needs real `.gitignore` pattern semantics (negation, `**`, directory-only patterns) rather than simple substring/prefix checks | Recommend only if the hand-rolled path-vs-gitignore-line comparison in 16i's test turns out to need real glob semantics; for the currently enumerated flat `.devflow/*` paths a simple prefix/line-match check is likely sufficient and simpler — planner's call |
| `project-root` crate (neilwashere/rust-project-root) | 2021, ~26k weekly downloads | NOT recommended for 16f: it is scoped to finding the nearest `Cargo.lock`, not a `.devflow/`-marker directory. Documented for completeness only. | Do not use — hand-roll the walk-up loop instead (see Code Examples) |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled `.devflow/` walk-up loop (16f) | `project-root` crate | Rejected: that crate is Cargo.lock-specific; DevFlow's marker is `.devflow/`, and the walk-up loop is ~10 lines — a dependency adds no value here |
| Hand-rolled gitignore-line check (16i) | `ignore` crate | `ignore` is the correct answer if pattern semantics (negation, globs) matter; if 16i only needs "is this literal path/prefix listed," a dependency is unwarranted per the project's stated aversion to unnecessary abstraction — planner should default to hand-rolled unless the enumerated paths need real glob matching |
| External command-based notify verification (16j) | A push-notification SDK with delivery receipts (e.g. a dedicated ntfy client library) | Rejected: websearch confirms neither ntfy nor desktop `notify-send` exposes a first-party delivery-receipt API; the verifiable signal has to come from DevFlow's own side (persistent `devflow status` state), not the notify transport |

**Installation:**
```bash
# In crates/devflow-core/Cargo.toml, add to [dependencies] (workspace-pinned):
# toml.workspace = true
# and to the root Cargo.toml [workspace.dependencies]:
# toml = "1"   # measured resolve: v1.1.3 (2026-07-17)
cargo add toml -p devflow-core
```

**Version verification:** `cargo add toml -p devflow-core --dry-run` was run at plan-write
time (2026-07-17) and resolves to **v1.1.3** — this supersedes the earlier websearch-sourced
~0.9.x figure (the websearch pass could not reach crates.io directly: direct `curl` to
`crates.io/api/v1/crates/toml` returned **HTTP 403** per crates.io's data-access policy,
requiring a compliant User-Agent header). The plan pins `toml = "1"` (semver-compatible with
v1.1.3); 16-02's blocking-human legitimacy checkpoint re-confirms the exact pin at execution
time before the `Cargo.toml` edit is committed.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `toml` | crates.io | ~11 yrs (first published 2014-11-11) | 13.4M/wk | github.com/toml-rs/toml | OK | Approved — required for D-03's `devflow.toml` |
| `ignore` | crates.io | ~9 yrs (since 2016-10-30) | 2.5M/wk | github.com/BurntSushi/ripgrep (crates/ignore) | OK | Approved only if 16i needs real gitignore glob semantics (see Standard Stack) — otherwise not needed |
| `project-root` | crates.io | ~5 yrs (since 2021-02-11) | 26k/wk | github.com/neilwashere/rust-project-root | OK | **Not recommended** — wrong marker semantics (Cargo.lock-specific), do not add |
| `notify-rust` | crates.io | ~11 yrs (since 2015-06-04) | 227k/wk | github.com/hoodie/notify-rust | OK | Not required — 16j's design conclusion is that no notify *library* fixes the verifiability gap; DevFlow already shells out to an operator-configured command via `DEVFLOW_GATE_NOTIFY_CMD` (`gates.rs`), which should stay a plain command string, not become a Rust-native notification dependency |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

*All four packages above were discovered via WebSearch/training knowledge (not
Context7/official-docs fetch in this session) and are tagged `[ASSUMED]` regardless of
their clean `[OK]` registry verdict, per the package-name-provenance rule. Only `toml` is
actually being added as a new dependency; the planner should gate its `Cargo.toml` edit
behind a `checkpoint:human-verify` task confirming the exact version pin before committing
it, per the ASSUMED-package protocol.*

## Architecture Patterns

### System Architecture Diagram

```
                 ┌─────────────────────────────────────────────────────────┐
                 │                  devflow-cli (main.rs)                  │
                 │  clap Command/Subcommand → project_root(path) resolve  │◄── 16f: walk-up
                 │       (currently: canonicalize only, no walk-up)        │    fix applies
                 └───────────────┬─────────────────────────────────────────┘    here for
                                 │                                               EVERY arm
                                 ▼
                 ┌─────────────────────────────────────────────────────────┐
                 │  advance() — per-phase lock → load_state → evaluate    │
                 │  agent_result::evaluate_agent_result                    │
                 │    Layer 1: DEVFLOW_RESULT marker / is_error  ◄─────────┼── 16a: needs a
                 │    Layer 2: exit code + commit-count fallback           │   Layer-0 EXTERNAL
                 │  (both trust the agent process; no independent check)  │   verification probe
                 └───────────────┬─────────────────────────────────────────┘   inserted before
                                 │ success/failed/verdict                       Layer 1 for plans
                                 ▼                                              with no repo-diff
                 ┌─────────────────────────────────────────────────────────┐   success signal
                 │  transition() / handle_*_outcome() / run_gate()         │
                 │   - hooks_for_transition(from,to) → run_checkout_hooks  │
                 │   - launch_stage() → cleanup_phase_files() ◄────────────┼── 16b: this wipe
                 │                    → spawn_monitor()                    │   happens BEFORE a
                 └───────────────┬─────────────────────────────────────────┘   human can inspect
                                 │                                              the prior stage's
                                 ▼                                              capture on a
                 ┌─────────────────────────────────────────────────────────┐   false-positive
                 │        External agent CLI (Claude / Codex / …)          │
                 │  runs prompt::stage_prompt(stage, phase):               │
                 │   Ship stage → /gsd-code-review {N} (single generalist  │◄── 16d: replace with
                 │   pass, standard depth) → Critical gate → /gsd-ship {N} │   deep + multi-angle
                 └───────────────┬─────────────────────────────────────────┘   (D-01/D-02)
                                 │ DEVFLOW_RESULT / REVIEW.md
                                 ▼
                 ┌─────────────────────────────────────────────────────────┐
                 │  handle_ship_outcome() → run_gate(Ship, "approve merge?")│
                 └───────────────┬─────────────────────────────────────────┘
                    GateAction::Advance
                                 ▼
                 ┌─────────────────────────────────────────────────────────┐
                 │  finish_workflow()                                       │
                 │   run_checkout_hooks(hooks_after_ship())                 │
                 │     = [VersionBump, BranchCleanup]  ◄─────────────────── │── 16k: NO MERGE
                 │     (GitFlow::feature_finish exists in git.rs but is     │   HOOK IS WIRED
                 │      never called from this path)                       │   HERE. VersionBump
                 │   clear_state() → events::emit("workflow_finished")     │   also runs against
                 └───────────────┬─────────────────────────────────────────┘   the WRONG checkout
                                 │                                             in worktree mode.
                                 ▼
                 ┌─────────────────────────────────────────────────────────┐
                 │  gates::fire_gate_notify() — shells out to               │
                 │  DEVFLOW_GATE_NOTIFY_CMD, checks only the command's own  │◄── 16j: exit-code
                 │  exit code (fire-and-forget)                             │   success ≠ human
                 └─────────────────────────────────────────────────────────┘   actually notified

External stores read/written throughout: `.devflow/state-{NN}.json`, `.devflow/gates/`,
`.devflow/events.jsonl` (16h reads this), `.devflow/phase-{NN}-{stdout,exit}` (16b
retains history here instead of wiping), `.gitignore` (16i asserts coverage of all of the
above), operator docs (README/ARCHITECTURE/CONTRIBUTING/OPERATIONS — 16c cross-checks
these against source).
```

### Recommended Project Structure

No new top-level directories are needed — Phase 16 extends existing modules. Suggested
new/changed files within `crates/devflow-core/src/`:

```
crates/devflow-core/src/
├── config.rs           # extend: devflow.toml struct + loader (D-03), keep
│                        #   GitFlowConfig hardcoded default unchanged
├── verify.rs            # NEW (16a): external post-condition probe runner —
│                        #   reads a PLAN.md-declared command, runs it, feeds
│                        #   result into evaluate_agent_result as a new Layer 0
├── agent_result.rs      # extend: cleanup_phase_files → archive_phase_files
│                        #   (16b: keep last N under .devflow/history/phase-NN/)
├── doc_check.rs          # NEW (16c/16i): grep-and-cross-reference checker +
│                        #   allowlist file loader, exercised only by #[test]
├── prompt.rs            # extend: ship_stage_prompt gains the angle list +
│                        #   capability-conditional fan-out instruction (16d)
├── hooks.rs             # FIX (16k): wire a merge hook into hooks_after_ship()
│                        #   BEFORE VersionBump; version.rs's commit-count must
│                        #   run post-merge against develop, not the worktree tip
├── gates.rs              # extend: fire_gate_notify gains a verifiable-delivery
│                        #   contract or a persistent status.rs indicator (16j)
└── history.rs            # NEW (16h): correlate events.jsonl + retained
                          #   REVIEW.md/capture history into one view
```

### Pattern 1: Shared project-root walk-up resolver (16f)

**What:** Replace `fn project_root(project: PathBuf)` (currently a bare canonicalize) with
a resolver that, when the given path doesn't directly contain `.devflow/`, walks
`Path::parent()` upward looking for it — mirroring `git rev-parse --show-toplevel`'s
`.git`-search semantics — before falling back to the originally-given path unchanged (so a
genuinely new/idle project, with no `.devflow/` anywhere, still works exactly as today).
**When to use:** Every CLI subcommand that currently calls `project_root(project)?` in
`main.rs` (`Start`, `Advance`, `Gate::*`, `Logs`, `Parallel`, `Sequentagent`, `Reference`,
`Cleanup`, `Status`, `List`, `Recover`, `Test`, `Doctor`) — apply it once, in the shared
function, not per call site.
**Example:**
```rust
// Illustrative shape — not copied from any external source (hand-rolled per
// Standard Stack's decision against the Cargo.lock-specific project-root crate).
fn project_root(project: PathBuf) -> Result<PathBuf, CliError> {
    let start = project
        .canonicalize()
        .map_err(|err| CliError::Message(format!("failed to resolve project path: {err}")))?;
    let mut probe = start.as_path();
    loop {
        if probe.join(".devflow").is_dir() {
            return Ok(probe.to_path_buf());
        }
        match probe.parent() {
            Some(parent) => probe = parent,
            // No .devflow/ found anywhere above — behave exactly as before
            // (idle-project case; commands that create .devflow/ still work).
            None => return Ok(start),
        }
    }
}
```
Note: this changes behavior only when `.devflow/` exists in an ancestor directory (i.e.
exactly the worktree-run case in 16f's incident) — a project with no `.devflow/` at all
is unaffected, and a project where `project` already directly contains `.devflow/` returns
immediately on the first iteration.

### Pattern 2: External post-condition "Layer 0" verification (16a)

**What:** A PLAN.md-declarable command that DevFlow itself executes after a stage's agent
process exits, independent of `DEVFLOW_RESULT`/exit-code/commit-count — its own exit code
(and optionally stdout content match) becomes an authoritative signal that OUTRANKS Layer
1/2 for plans that declare it.
**When to use:** Plans whose real success condition is an external state change with no
repo-diff evidence (the crates.io-publish plan is the motivating case) — NOT a
general-purpose replacement for the existing Layer 1/2 evaluation, which stays the default
for ordinary code-changing plans.
**Example (registry-probe shape, informed by the crates.io sparse-index research above):**
```rust
// Illustrative — the actual command comes from the PLAN.md-declared contract,
// DevFlow just runs it and checks its exit status (and optionally its stdout).
fn run_external_verification(cmd: &str, project_root: &Path) -> Result<bool, VerifyError> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(project_root)
        .output()?;
    Ok(output.status.success())
}
// A concrete probe for the crates.io incident, usable as a PLAN.md-declared
// verification command (no API key, no rate limit per the sparse-index docs):
// curl -sf https://index.crates.io/de/vf/devflow | tail -1 | grep -q '"vers":"1.3.0"'
```

### Pattern 3: Retained per-stage capture history (16b)

**What:** Instead of `cleanup_phase_files` deleting `.devflow/phase-NN-{stdout,exit}`
outright before every new stage launch, rotate the outgoing stage's files into a
retained, bounded history (e.g. `.devflow/history/phase-NN/{stage}-{attempt}.stdout`,
keep last N per `devflow.toml`'s new retention knob) — deletion still happens, but only
once the retention window is exceeded, and only for entries beyond N.
**When to use:** Called from the same site as today's `cleanup_phase_files`
(`launch_stage`, `main.rs:634`) — the call site doesn't move, only what it does.
**Example:**
```rust
// Illustrative rotation shape — keep this in agent_result.rs alongside the
// existing stdout_path/exit_code_path helpers, which stay unchanged.
pub fn archive_phase_files(project_root: &Path, phase: u32, stage: Stage, retain: usize) {
    let history_dir = devflow_dir(project_root).join("history").join(format!("phase-{phase:02}"));
    let _ = std::fs::create_dir_all(&history_dir);
    let stamp = format!("{stage}-{}", unix_now());
    for (src, ext) in [
        (stdout_path(project_root, phase), "stdout"),
        (exit_code_path(project_root, phase), "exit"),
    ] {
        let _ = std::fs::rename(&src, history_dir.join(format!("{stamp}.{ext}")));
    }
    prune_history(&history_dir, retain); // keep only the newest `retain` entries
}
```

### Pattern 4: Capability-conditional multi-angle Ship review prompt (16d/D-01/D-02)

**What:** One shared prompt string (still built by `prompt.rs::ship_stage_prompt`) that
enumerates the angle list (doc-accuracy, security/leaked-data, CI/build correctness,
external-state claims, +1 generalist deep pass) and instructs the agent: "if your harness
supports parallel subagents/Task tool, dispatch one per angle in parallel; otherwise run
each angle as a focused sequential pass — merge every angle's findings into one
`REVIEW.md`, deduplicated." This mirrors the Phase 14 post-ship review's proven
finder-angle → dedup → verify shape (`14-REVIEW.md` frontmatter: `method: 8 finder angles
… → dedup → 1-vote verify per candidate`), reusing a pattern already validated in this
project rather than inventing a new review methodology.
**When to use:** Only inside `ship_stage_prompt` — the Validate/Code/Define/Plan prompts
are untouched.
**Example (prompt-string composition, following the existing `format!`/const idiom in
`prompt.rs`):**
```rust
// Source: this project's own crates/devflow-core/src/prompt.rs idiom
const SHIP_REVIEW_ANGLES: &str = "\
- doc-accuracy cross-reference (do documented claims match source?)\n\
- security / leaked-data (does anything commit secrets, session data, telemetry?)\n\
- CI/build correctness (can a failing step still report green?)\n\
- external-state claims (does the diff assert something about state DevFlow \
  doesn't actually control — merges, tags, deletions — that isn't actually true?)\n\
- one generalist deep pass";
// ship_stage_prompt embeds SHIP_REVIEW_ANGLES plus a conditional
// parallel-subagent-or-sequential-pass instruction before /gsd-code-review {N}.
```

### Anti-Patterns to Avoid
- **Trusting agent self-report as the sole signal for external-only plans:** the exact
  failure this phase exists to fix (16a) — always pair with an independent, DevFlow-run
  check when there is no repo-diff evidence.
- **Wiping diagnostic state before a human can see it:** `cleanup_phase_files` running on
  every launch (16b) — retain, don't delete-then-recreate, for anything that might need
  post-hoc forensics.
- **Treating a fire-and-forget shell command's exit code as "the operator was notified"**
  (16j) — an exit-0 notify command proves the command ran, not that a human saw anything.
- **A single generalist review pass for a large accumulated diff** (16d) — proven (four
  times, in Phase 15) to miss distinct findings that narrower, focused passes catch.
- **Adding a hook to `hooks_for_transition`/`hooks_after_ship` without auditing what
  *isn't* there** — 16k's bug is an *absence*, not a broken present-hook; when reviewing
  hook wiring, explicitly check "what should run here that currently doesn't," not only
  "does the existing hook work."

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TOML parsing/serialization for `devflow.toml` (D-03) | A custom TOML tokenizer | `toml` crate + `serde::Deserialize` | TOML has real edge cases (nested tables, arrays of tables, string escaping) — the existing `field_for`/`parse_section_header` hand-rolled TOML editing in `version.rs` is a narrow value-replacement tool, not a general parser, and should not be extended into one |
| `.gitignore` glob semantics (16i, only if needed) | A hand-rolled glob matcher for `**`, negation (`!`), directory-only patterns | `ignore` crate | Gitignore pattern matching has subtle, well-documented edge cases (BurntSushi's own crate exists because getting this right is nontrivial); only reach for it if 16i's enumerated paths turn out to need real glob semantics beyond flat literal/prefix matches |
| Registry-publish verification (16a, crates.io case) | A hand-rolled crates.io API client with auth/retry/backoff | The sparse index (`index.crates.io/<name>`) via a plain `curl`/`reqwest` GET, no auth, no rate limit per crates.io's own docs | The sparse index exists specifically so tools don't need a full API client for simple existence/version checks |
| Cross-attempt history correlation (16h) | A new state store/database | `events.jsonl` (already schema-versioned, already has `last_events_by_phase`) + retained `REVIEW.md` history from 16b | The canonical reference already flags this explicitly: "16h should derive from this, not invent a new store" |

**Key insight:** Every "don't hand-roll" item above already has a comparably-scoped
existing pattern in this codebase (the TOML value-replacement idiom, the events.jsonl
schema, the sparse-index docs) — the risk in this phase specifically is *scope creep*
into building a bigger general-purpose tool than the one incident that motivated each
item actually requires.

## Runtime State Inventory

> Included because 16k/16f touch runtime git/state behavior and the phase is explicitly
> about correcting prior state-mutation bugs, even though this is not a rename/refactor
> phase per se.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `.devflow/state-{NN}.json` (per-phase, gitignored), `.devflow/gates/` response files, `.devflow/events.jsonl` — none reference a renamed identifier; no data migration needed for this phase | None — 16b/16h read/extend these formats, no schema-breaking change implied by the scope items |
| Live service config | Primary checkout's git refs/tags: 16k's incident already left `v1.2.183` tagged on an unrelated docs commit in the live repo, and a bogus series of CHANGELOG entries (1.2.175/176/179 "Released phase via DevFlow") — **these are pre-existing corrupted artifacts in THIS repo's actual git history**, not hypothetical | Out of this phase's code-fix scope, but flag for the planner: a cleanup task (delete the bogus tags, or at minimum document them as known-bad in CHANGELOG) may be warranted so 16k's fix isn't validated against an already-corrupted tag sequence |
| OS-registered state | None found — no cron/systemd/launchd/pm2 registration exists in this codebase; `cron-instructions*.json` is DevFlow's own file-based self-re-run record, not an OS scheduler registration | None |
| Secrets/env vars | `DEVFLOW_GATE_NOTIFY_CMD`, `DEVFLOW_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` — D-03 explicitly leaves these untouched (env var still wins over the new `devflow.toml`) | None — no renaming, only additive precedence (env > file > default) |
| Build artifacts | None applicable — no compiled/installed artifact carries a renamed string in this phase | None |

**Verified live in this repo:** the bogus `v1.2.183` tag and 1.2.175/176/179 CHANGELOG
entries referenced by 16k's evidence are real, current state in this project's git
history (per `.planning/STATE.md`'s 2026-07-17 decision entry) — not a hypothetical to
reproduce.

## Common Pitfalls

### Pitfall 1: Treating `BranchCleanup`'s `ok=true` as proof of a successful ship
**What goes wrong:** `run_checkout_hooks` logs `hook_run {"hook": "BranchCleanup", "ok":
outcome.is_ok()}` — but `branch_cleanup` (`hooks.rs:95`) returns `Ok(())` even when it
deletes nothing (branch absent, or "not fully merged" — both fail-soft-logged as `warn!`
internally, with the *outer* `Result` still `Ok`). A reviewer or a new checker built for
16k could misdiagnose this as "the hook itself is broken" when the real defect is
upstream: nothing ever merged the branch, so `BranchCleanup` correctly has nothing to
clean up.
**Why it happens:** `HookError` only models genuine I/O/git-command failures, not
"the operation was a no-op." The hook's return type conflates "ran without an OS-level
error" with "did the thing it exists to do."
**How to avoid:** When fixing 16k, distinguish "hook executed without error" from "hook's
intended effect actually happened" at the event-emission layer — e.g. emit a
`"branches_deleted": ["…"]`/`"branches_deleted": []` field alongside `ok`, so
`events.jsonl` (and 16h's history view) can tell a real no-op apart from a silent failure.
**Warning signs:** Any hook whose `Result<(), HookError>` is `Ok(())` on a codepath that
also `warn!`s — that's the same "success reported, nothing actually happened" shape as the
original incident.

### Pitfall 2: Fixing 16k by adding a merge call without reordering VersionBump
**What goes wrong:** If a merge hook is inserted into `hooks_after_ship()` naively
(e.g. appended after `VersionBump`), `VersionBump`'s `compute_version` (which counts git
tags and commits-since-last-tag via `git rev-list --count`) will still run against the
pre-merge tree — reproducing the same "wrong checkout"/nonsense-version-number failure
mode from a different angle, because the merge hasn't landed yet when the version is
computed.
**Why it happens:** `hooks_after_ship()` returns a `Vec<Hook>` executed in list order by
`run_checkout_hooks`'s `for hook in batch` loop (`main.rs:964`) — order matters and is
currently `[VersionBump, BranchCleanup]`, i.e. version-then-cleanup, with no merge step at
all.
**How to avoid:** The merge hook must run FIRST in the terminal batch: `[Merge,
VersionBump, BranchCleanup]` — so `VersionBump` computes against the post-merge `develop`
HEAD, exactly like the existing `hooks_for_transition(Validate, Ship)` ordering
(`[DocsUpdate, ChangelogAppend]`) already establishes docs before changelog before any
version stamp.
**Warning signs:** A live/dogfood run where the tagged version doesn't match the actual
merged commit count on `develop` — re-verify with `git log --oneline -1 <tag>` against
`git log --oneline -1 develop` after any 16k fix, before considering it done.

### Pitfall 3: A generic "grep every identifier in docs" checker (16c) produces unusable noise
**What goes wrong:** D-06 explicitly rejected generic value-extraction from prose because
it produces false positives that erode trust in the checker (an existence-only check
also explicitly would NOT have caught the RUST_LOG-default incident that motivated 16c in
the first place — the checker needs BOTH an existence layer and a hand-maintained pinned-
claims layer).
**Why it happens:** Doc prose describing *behavior* ("defaults to X") isn't
mechanically checkable against source without understanding semantics — only mechanically
checkable against source for *existence* (does the identifier/flag/path appear at all).
**How to avoid:** Build exactly the two layers D-06 locked: (1) existence-only regex/grep
cross-reference for every env var/flag/subcommand/path named in scoped docs, and (2) a
short, explicit, hand-written list of `(doc claim, source assertion)` pairs — e.g. `assert!
default_env_filter_level() == Level::ERROR` alongside the doc text it's pinned to — grown
incrementally as new claims are documented, never auto-derived from prose.
**Warning signs:** A checker PR that tries to regex-extract "defaults to `<value>`"
patterns generically from markdown — that's the rejected approach; catch it in review.

### Pitfall 4: `.gitignore` invariant test passing today gives false confidence about tomorrow
**What goes wrong:** As verified in this research, the CURRENT `.gitignore` already covers
every enumerated `.devflow/`-writing path — so a naive 16i test written against today's
path list will pass immediately and look "done," without actually preventing the *next*
rename-without-gitignore-update (the exact regression class that caused the original
incident).
**Why it happens:** A hardcoded list of paths-to-check, rather than a derivation from
source's actual path-constructor functions, degrades into exactly the kind of stale
snapshot that let the original leak go undetected.
**How to avoid:** Per the canonical reference's explicit instruction, the 16i test "must
enumerate these from source, not from a hardcoded list" — e.g. call each module's public
path-constructor function (`events::events_path`, `workflow::state_path`,
`gates::gates_dir`, `agent_result::stdout_path`/`exit_code_path`, `lock::lock_path`,
`ship::cron_instructions_path`, monitor's pid-file path) at test time and assert each
against `.gitignore`, so a NEW path-writing function added later without a matching
constructor call in the test is a compile error or an obviously-incomplete diff, not a
silent gap.
**Warning signs:** A 16i test that imports `once_cell`/`lazy_static` with a literal
`Vec<&str>` of path strings instead of calling the modules' own path functions.

## Code Examples

Verified patterns from this project's own source (no external library APIs are new
enough to this phase to need external code examples beyond the illustrative snippets
already given in Architecture Patterns above):

### Existing prompt-composition idiom to extend for 16d
```rust
// Source: crates/devflow-core/src/prompt.rs:52-72 (read this session)
fn ship_stage_prompt(phase: u32) -> String {
    let code_review = format!("/gsd-code-review {phase}");
    let ship = format!("/gsd-ship {phase}");
    format!(
        "Run the Ship stage in two steps:\n\
        \n\
        1. Run `{code_review}` (non-interactive). This writes a `REVIEW.md` \
        artifact with severity-classified findings.\n\
        2. Check `REVIEW.md` for the Critical-severity gate:\n\
        ...\n\
        {COMPLETION_PROTOCOL}"
    )
}
```
16d extends step 1's instruction text (angle list + conditional fan-out) without changing
the two-step Critical-gate structure that already exists and is already snapshot-tested
(`ship_prompt_sequences_code_review_before_ship`,
`ship_prompt_defines_critical_gate_and_review_failed_contract` in `prompt.rs`'s own test
module) — new snapshot assertions should extend that same test module.

### Existing hook-batch execution to extend for 16k
```rust
// Source: crates/devflow-core/src/hooks.rs:76-86 (read this session)
pub fn hooks_for_transition(from: Stage, to: Stage) -> Vec<Hook> {
    match (from, to) {
        (Stage::Validate, Stage::Ship) => vec![Hook::DocsUpdate, Hook::ChangelogAppend],
        _ => Vec::new(),
    }
}

pub fn hooks_after_ship() -> Vec<Hook> {
    vec![Hook::VersionBump, Hook::BranchCleanup]   // <-- 16k: missing a merge hook here
}
```

### Existing merge primitive already implemented, currently only exercised by tests
```rust
// Source: crates/devflow-core/src/git.rs:71 (signature read this session;
// exercised by feature_finish_merges_into_develop_and_deletes test at git.rs:508)
pub fn feature_finish(&self, phase: u32) -> Result<String, GitError> {
    // checks out develop, merges feature/phase-NN --no-ff, deletes the branch
}
```
This function already exists and is already unit-tested — 16k's fix is very likely to be
"call `feature_finish` (or a hook wrapping it) from `hooks_after_ship`'s terminal path,"
not "implement merge logic from scratch."

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| No config file at all (v2.0.0 architecture decision, 2026-06-19) | Minimal `devflow.toml` for Phase 16's new knobs only, env > file > default precedence (D-03) | This phase (2026-07-17 decision) | `config.rs`'s "no config file" doc comment must be corrected; `DEVFLOW_*` env vars remain the override layer, unaffected |
| Single-pass, standard-depth `/gsd-code-review` at Ship (introduced 13-02) | Deep-mode + multi-angle (capability-conditional parallel-or-sequential) review at Ship (16d) | This phase | Same two-step Critical-gate structure retained; only the review's own depth/breadth changes |
| `.devflow/phase-NN-{stdout,exit}` clobbered on every stage launch (14a refactor) | Retained, bounded per-stage capture history (16b) | This phase | Enables post-hoc diagnosis of a false-positive self-report, which was impossible during the actual Phase 15 incident |
| Review only at whole-phase Ship time | + incremental per-plan/per-wave review (16e) | This phase | Drift caught earlier; exact gating semantics are D-04 (planner's discretion) |

**Deprecated/outdated:**
- The "no config file" stance (2026-06-19, reaffirmed 2026-07-08 via `.devflow.yaml` decoy
  removal) is explicitly and knowingly reversed by D-03 for a minimal file — this is not
  an oversight, it's a recorded, deliberate reopening.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `toml` crate is the correct/only serious choice for `devflow.toml` (D-03); exact version measured at plan-write time as **v1.1.3** (`cargo add toml -p devflow-core --dry-run`, 2026-07-17), superseding the earlier ~0.9.x websearch estimate | Standard Stack | RESOLVED — no longer an open assumption on correctness or version. The crate is unambiguously the standard choice; the version was verified by live `cargo add --dry-run` and the plan pins `toml = "1"` (semver-compatible). 16-02's blocking-human checkpoint re-confirms the pin before commit |
| A2 | `ignore` crate would be the right tool IF 16i needs real gitignore glob semantics | Standard Stack / Don't Hand-Roll | Low risk — this is explicitly conditional advice, not a locked recommendation; if the planner's 16i design only needs flat literal-path matching (which the current enumerated path set suggests), no new dependency is needed at all |
| A3 | ntfy/desktop-notify provide no first-party delivery-receipt API (16j design conclusion) | Phase Requirements table / Don't Hand-Roll | Medium — sourced from a single websearch pass, not exhaustive vendor-doc review; if a delivery-receipt mechanism does exist for the operator's specific notify channel, 16j's design could use it instead of falling back to a DevFlow-side persistent indicator. Recommend the planner confirm the operator's actual `DEVFLOW_GATE_NOTIFY_CMD` target (ntfy? desktop? something else?) before finalizing 16j's approach |
| A4 | `clap`'s documented trailing-positional-plus-subcommand ambiguity is the exact mechanism behind the `devflow gate approve 15 ship` footgun | Phase Requirements table (16g) | Low — directly reproduced by reading `GateCmd::Approve`'s field order in `main.rs:234-247` (two bare positionals: `phase`, then `project` after keyword flags) against clap's own documented behavior; high confidence this is correct, but the exact clap version's exact parsing algorithm wasn't independently fetched from clap's own docs in this session |

**If this table is empty:** N/A — see above; all four assumptions are LOW-to-MEDIUM risk
and none block planning, but A1 and A3 in particular should be spot-checked (a
`cargo add --dry-run` and a one-line confirmation of the operator's real notify channel,
respectively) before being treated as locked in the plan.

## Open Questions

1. **What exactly should the Merge hook do about a PR that's already open (the live 16k
   incident left PR #7 open when the phase was manually shipped)?**
   - What we know: `GitFlow::feature_finish` does a local `git merge --no-ff` into
     `develop` — it has no concept of a GitHub PR at all; the actual Phase 15 incident
     involved a real PR (#7) that the ship agent had opened via `gh pr create` (per
     `/gsd-ship`'s own workflow), which the terminal hook path never merged or closed.
   - What's unclear: whether 16k's fix should (a) call `feature_finish`'s local merge
     regardless of PR state, (b) shell out to `gh pr merge` when a PR exists for the
     branch, or (c) detect and refuse to double-merge if a PR was already merged
     out-of-band (as literally happened in this incident's manual recovery).
   - Recommendation: the planner should design 16k's fix to be idempotent and PR-aware —
     check whether the feature branch is already merged into `develop` (or the PR is
     already merged/closed) before attempting either a local merge or a `gh pr merge`
     call, so re-running a fixed `finish_workflow` against an already-manually-shipped
     phase (like the actual current state of Phase 15) is a safe no-op, not a duplicate
     merge attempt.

2. **What does "verifiable operator notification" (16j) mean operationally, given no
   notify transport offers delivery receipts?**
   - What we know: the notify command itself can only report its own exit code; ntfy/
     desktop-notify don't expose delivery confirmation (websearch, MEDIUM confidence).
   - What's unclear: whether "verifiable" should mean (a) DevFlow polls/blocks until the
     gate is actually answered with a louder/more persistent local indicator (e.g. a
     `devflow status` banner that repeats/escalates the longer a gate sits open), or (b)
     integrating a genuinely receipted channel (e.g. requiring the notify command to be
     something with delivery confirmation, which is an operator configuration concern,
     not something DevFlow's code can force).
   - Recommendation: treat 16j's fix as (a) — a persistent, escalating `devflow status`
     indicator for an open gate — since that's the one piece entirely within DevFlow's
     own control, independent of whatever `DEVFLOW_GATE_NOTIFY_CMD` the operator has
     configured.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo`/Rust toolchain | Entire phase (D-05's checkers are `#[test]` fns) | ✓ (workspace builds; `rust-toolchain.toml` pins stable + clippy/rustfmt) | stable (per `.devcontainer/devcontainer.json`, image `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`) | — |
| `git` CLI | 16f/16k (all git-flow operations shell out to `git`) | ✓ (used extensively by existing `git.rs`) | — | — |
| Network access to `crates.io`/`index.crates.io` | 16a's example crates.io verification probe | ✗ in THIS research session — direct `curl` to `crates.io/api/v1/crates/toml` returned HTTP 403 (data-access policy requires a compliant User-Agent) | — | Use the sparse index (`index.crates.io/<name>`) with a proper User-Agent header, or `cargo search`/`cargo info` (delegates auth/UA handling to `cargo` itself) rather than raw `curl` |
| A notify transport (ntfy, `notify-send`, or operator-defined `DEVFLOW_GATE_NOTIFY_CMD`) | 16j | Not probed (operator-specific, out of this repo's control) | — | 16j's recommended fallback (persistent `devflow status` indicator) works even with `DEVFLOW_GATE_NOTIFY_CMD` unset |

**Missing dependencies with no fallback:** none — every gap above has a documented
fallback.
**Missing dependencies with fallback:** direct unauthenticated `crates.io` REST API access
(use the sparse index or `cargo` itself instead, per Environment Availability above).

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (plain Rust `#[test]`, no `nextest`/external harness) |
| Config file | none — CI (`.github/workflows/ci.yml`) runs `cargo test` directly; devcontainer CI-parity (`.github/workflows/devcontainer.yml`) runs `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings && cargo fmt --check` under `set -e` (this `set -e` was itself one of Phase 15's fixes — already verified present) |
| Quick run command | `cargo test -p devflow-core <module>::` (scope to the touched module during iteration) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| 16k | Terminal path actually merges feature branch into develop before/around VersionBump | unit + integration | `cargo test -p devflow-core hooks_after_ship` (extend existing `after_ship_runs_version_and_cleanup` in `hooks.rs`) | ❌ Wave 0 — needs a new merge-inclusive assertion |
| 16a | A declared external-verification command is run and its result outranks Layer 1/2 for plans that declare it | unit | `cargo test -p devflow-core evaluate_agent_result` (new module, e.g. `verify.rs`) | ❌ Wave 0 |
| 16b | Prior stage's capture files survive the next stage's launch, up to retention N | unit | `cargo test -p devflow-core cleanup_removes_phase_files` (extend/replace in `agent_result.rs`, which currently only asserts *removal*, not retention) | ✅ existing test asserts current (to-be-changed) behavior — must be updated, not just added to |
| 16c | Every doc-referenced identifier/flag/env-var/path exists in source; pinned semantic claims match source behavior | unit | `cargo test -p devflow-core doc_claims_exist_in_source` (new module) | ❌ Wave 0 |
| 16d | Ship prompt carries the angle list + conditional fan-out instruction | unit (snapshot) | `cargo test -p devflow-core ship_prompt` (extend `prompt.rs`'s existing test module) | ✅ existing snapshot tests to extend |
| 16e | Per-wave/per-plan incremental review fires per D-04's planner-chosen gating semantics | unit + manual | TBD once D-04 is resolved by the planner | ❌ Wave 0 (depends on planner design) |
| 16f | Every subcommand resolves `project_root` via walk-up when run from inside a `.devflow`-descendant directory | unit | `cargo test -p devflow project_root_walks_up` (new test in `main.rs`'s test module) | ❌ Wave 0 |
| 16g | Legacy-state WARN hints at `recover --clean`; `gate approve <phase> <stage>` positional collision is caught/hinted | unit | `cargo test -p devflow-core migrate_legacy_state` (extend `workflow.rs`) + `cargo test -p devflow gate_approve_arg_parsing` (new, `main.rs`) | Partial — legacy-state test file exists, footgun test does not |
| 16h | `events.jsonl` + retained REVIEW.md history correlate into one cross-attempt view | unit | `cargo test -p devflow-core history` (new module) | ❌ Wave 0 |
| 16i | Every `.devflow/`-writing path (enumerated from source, not hardcoded) is covered by `.gitignore` | unit | `cargo test -p devflow-core gitignore_covers_all_devflow_paths` (new, per Pitfall 4's guidance) | ❌ Wave 0 |
| 16j | An open gate produces a persistent, escalating `devflow status` indicator independent of the notify command's exit code | unit + manual | `cargo test -p devflow status_shows_pending_gate_prominently` (extend `main.rs`'s `status` tests) | Partial — `status` has some gate-pending tests already; escalating-indicator behavior is new |

### Sampling Rate
- **Per task commit:** `cargo test -p devflow-core <touched module>::` and `cargo test -p
  devflow <touched module>::` (whichever crate the task's files live in)
- **Per wave merge:** `cargo test --workspace && cargo clippy --workspace -- -D warnings
  && cargo fmt --check` (mirrors CI exactly)
- **Phase gate:** Full suite green before `/gsd-verify-work`; per D-05, the 16c/16i
  checkers being `#[test]` functions means this same command already enforces them — no
  separate invocation needed

### Wave 0 Gaps
- [ ] `verify.rs` (or equivalent) — new module + tests, covers 16a
- [ ] `doc_check.rs` (or equivalent) — new module + tests + allowlist file format, covers
      16c/16i
- [ ] `history.rs` (or equivalent) — new module + tests, covers 16h
- [ ] Extended `hooks.rs` merge-hook test (replacing/extending
      `after_ship_runs_version_and_cleanup`), covers 16k
- [ ] Extended `agent_result.rs` retention test (replacing the current
      delete-on-cleanup assertion), covers 16b
- [ ] New `main.rs` tests for walk-up resolution (16f) and the gate-approve positional
      footgun (16g)
- Framework install: none — `cargo test` is already fully configured; no new test
  framework/dependency needed

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | DevFlow has no authentication surface — single-operator local CLI |
| V3 Session Management | No | N/A |
| V4 Access Control | No | N/A — filesystem permissions are the only boundary, unchanged by this phase |
| V5 Input Validation | Partial — yes for 16a/16c | 16a's external-verification command comes from a PLAN.md the operator authored (trusted input, same trust level as existing GSD command construction); 16c's doc-scan must not execute anything it finds in docs, only grep/compare — no `eval`-like pattern should ever be introduced |
| V6 Cryptography | No | N/A — no new crypto surface |
| V-DevFlow-specific: command injection via shell-out | Yes | Every existing shell-out in this codebase (`docs_update`'s `Command::new("sh").arg("-c")`, `run_notify_command`, hooks' git invocations) already uses `Command::new`+`arg()` with structured args, not raw string interpolation into a single shell command built from untrusted input — 16a's new external-verification runner MUST follow the same pattern: the command string comes from a PLAN.md (operator-authored, already-trusted per this project's existing threat model), never from agent stdout or any other untrusted runtime source |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Agent-controlled string reaching a gate/notify context verbatim (13-06's multi-KB raw JSONL reaching a desktop notification) | Information Disclosure / Denial of Service (noise) | Already mitigated by `truncate_reason` (`main.rs:849`) — 16j's persistent-indicator design must reuse this same truncation, not bypass it by rendering the raw reason in a new status surface |
| A future 16a verification command sourced from agent output instead of the operator-authored PLAN.md | Tampering / command injection | Explicitly scope 16a's command source to PLAN.md content only (operator-trusted), never to agent stdout, exit-code text, or any other runtime-produced string — this is a design constraint the plan should assert as a test, not just a convention |
| Leaked runtime telemetry recommitted to git (the exact 16i-motivating incident: `.devflow/phase-07-stdout` with `session_id`/cost data sat in git history) | Information Disclosure | 16i's deterministic invariant test IS the standard mitigation here — verified this phase's `.gitignore` already covers all currently-enumerated paths; the test exists to keep it that way as new paths are added |

## Sources

### Primary (HIGH confidence — direct source reads this session)
- `crates/devflow-core/src/prompt.rs`, `hooks.rs`, `agent_result.rs`, `events.rs`,
  `gates.rs`, `git.rs`, `version.rs`, `recover.rs`, `workflow.rs`, `mode.rs`, `config.rs`
  — read in full or targeted sections this session
- `crates/devflow-cli/src/main.rs` — targeted sections (Cli/Command/GateCmd definitions,
  `advance`/`transition`/`finish_workflow`/`run_gate`/`status`/`project_root`,
  `launch_stage`) read this session
- `.gitignore`, `Cargo.toml` (root + both crates), `.github/workflows/ci.yml`,
  `.github/workflows/devcontainer.yml`, `.devcontainer/devcontainer.json` — read this
  session
- `.planning/phases/16-pipeline-reliability-hardening/16-CONTEXT.md`,
  `.planning/phases/16-pipeline-reliability-hardening/CONTEXT.md`, `.planning/STATE.md`
  — read this session (upstream inputs, not independently verified beyond their own
  content)
- `.planning/phases/14-parallel-safety-observability/14-REVIEW.md` — read (frontmatter)
  this session, confirms the 8-finder-angle → dedup → 1-vote-verify precedent

### Secondary (MEDIUM confidence — WebSearch cross-checked against training knowledge)
- [toml crate docs.rs](https://docs.rs/toml) / [crates.io: toml](https://crates.io/crates/toml) / [github.com/toml-rs/toml](https://github.com/toml-rs/toml) — version/serde-integration claims
- [crates.io data-access policy](https://crates.io/data-access) / [crates_io_api docs.rs](https://docs.rs/crates_io_api) / [crates-index crates.io](https://crates.io/crates/crates-index) — sparse-index and API-access mechanics for 16a's external verification probe
- [project-root docs.rs](https://docs.rs/project-root) — confirmed as Cargo.lock-specific, hence not recommended for 16f
- [ntfy docs: publish](https://docs.ntfy.sh/publish/) / [ntfy docs: FAQs](https://docs.ntfy.sh/faq/) / [ntfy docs: phone subscribe](https://docs.ntfy.sh/subscribe/phone/) — delivery-latency and no-delivery-receipt claims for 16j
- [clap Arg docs.rs](https://docs.rs/clap/latest/clap/struct.Arg.html) / [clap-rs/clap discussion #2260](https://github.com/clap-rs/clap/discussions/2260) / [clap-rs/clap issue #4815](https://github.com/clap-rs/clap/issues/4815) — trailing-positional/subcommand ambiguity claims for 16g
- gsd-tools `package-legitimacy check` registry queries (crates.io-backed) for `toml`, `ignore`, `project-root`, `notify-rust` — existence/age/downloads/repo signals in the Package Legitimacy Audit table

### Tertiary (LOW confidence)
- None retained as standalone claims — every websearch-sourced claim above was
  cross-referenced against at least one additional result in the same search and is
  tagged MEDIUM (`classify-confidence --provider websearch --verified`), not LOW.

## Metadata

**Confidence breakdown:**
- Standard stack: MEDIUM — the one new dependency (`toml`) is an uncontested standard
  choice, but its exact version wasn't fetched from crates.io directly in this session
  (403 on direct API access); confirm before locking the plan's `Cargo.toml` edit
- Architecture: HIGH — every scope item's root cause was located to specific,
  cited lines in the actual codebase and cross-checked against the incident
  descriptions in `16-CONTEXT.md`/`CONTEXT.md`
- Pitfalls: HIGH for 16k/16i/16c (directly reproduced from source); MEDIUM for 16j
  (websearch-sourced delivery-guarantee claims, single-pass verification)

**Research date:** 2026-07-17
**Valid until:** 2026-08-17 (30 days — this is an internal-codebase-driven phase; the
external-library portion, `toml`, is a decade-stable ecosystem staple unlikely to shift
meaningfully in that window)
