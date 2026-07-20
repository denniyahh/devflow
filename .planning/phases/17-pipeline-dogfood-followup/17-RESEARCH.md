# Phase 17: Pipeline Dogfood Follow-Up - Research

**Researched:** 2026-07-18
**Domain:** Rust CLI/library workflow engine — completion-signal classification, retry policy, preflight validation, build provenance
**Confidence:** HIGH (all four scope units verified directly against the current source tree; no speculative library research was needed — this phase adds no new dependencies)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**17a — Unknown-outcome policy**

- **D-01 (root defect):** `main.rs:854` classifies only `Failed | RateLimited` as failure, so `Unknown` falls through to the success arm at `main.rs:871` (whose comment states the behavior outright) and `Stage::Code => transition(..., Stage::Validate)` fires unconditionally. Broader than the retrospective recorded: `evaluate_layer3` (`agent_result.rs:610-620`) returns `Unknown` for the zero-commit "process gone, no commits" branch too, so a vanished agent that produced nothing also advances.
- **D-02 (fix locus): split at Layer 3 into typed outcomes.** `evaluate_layer3` returns distinct outcomes rather than one `Unknown`. Type-driven: the exhaustive match in `advance()` forces every future stage/outcome pair to be handled, which prevents this regression class rather than re-patching it. Composes with 17b's taxonomy.
- **D-03 (zero-commit policy): three-way, not binary.** Zero commits is NOT inherently failure — the originating incident was an external check that legitimately produced no code. Policy:
  1. zero commits + declared external post-condition **passed** → advance
  2. zero commits + declared external post-condition **failed** → fail (already works today)
  3. zero commits + **no declaration at all** → genuinely ambiguous what the stage did → treat as failure, notify human for review
- **D-04 (commits-present policy):** post-condition if declared, else gate for explicit human approval. Matches retrospective AC-3 literally and reuses Phase 16's Layer 0 rather than inventing a second mechanism.
- **D-05 (Layer 0 must be extended — REQUIRED for D-03):** 16a's Layer 0 exists (`agent_result.rs:627-690`) and is the right mechanism, but has two gaps that make D-03 unimplementable as-is:
  1. **Code-stage only** — `agent_result.rs:638` returns `None` unless `state.stage == Stage::Code`. Conflicts with D-06.
  2. **Passing probes are not affirmative evidence** — the docstring is explicit (lines 629-631): success "defers to the existing Layer 1/2/3 cascade", and the code only maps failures. A probe can veto, never vouch. Consequence: an external-only stage with zero commits still cannot succeed cleanly — it falls to Layer 2's Plan|Code commit gate or Layer 3 `Unknown`. This is exactly the originating incident.

  Phase 17 lifts the stage restriction and lets a passing declared probe count as affirmative completion evidence. The operator-approval mechanism (`TRUST_EXTERNAL_VERIFY_ENV` holding a reviewed JSON command array, with mismatch detection in both directions) is UNCHANGED — it is the security property that makes Layer 0 trustworthy and must not be relaxed.
- **D-06 (stage scope): every stage.** Define, Plan, Code, Ship all get the non-advance rule. Validate is already fail-safe via 13-05's verdict gating so is unaffected in practice, but a uniform rule leaves no stage-shaped hole.

**17b — Outcome taxonomy + retry policy**

- **D-07 (new outcomes):** add `ResourceKilled` (exit 137 — currently unhandled; `rg "137"` returns nothing workspace-wide) and `AgentUnavailable`. `RateLimited` and `Unknown` already exist (`agent_result.rs:41-50`), so the retrospective's proposed five-outcome set is three-quarters present.
- **D-08 (failure budget): separate counters.** Infrastructure outcomes (rate-limited, OOM-killed) do NOT increment `consecutive_failures`, the counter driving gate→abort. They get their own counter with its own ceiling. Rationale: spending the abort budget on conditions the agent never controlled aborts phases whose work was fine — the same false-signal family this phase exists to fix.
- **D-09 (retry): auto-resume `rate_limited` only.** Rate limits already have resume machinery (`cron-instructions-NN.json`); extending it is cheap and the recovery is unambiguous. Every other outcome gates — an OOM kill or a missing binary needs a human to change something, and auto-retrying a workload that will always exhaust memory burns agent time unobserved.
- **D-10 (evidence): structured record on every terminal decision.** Replaces `reason: null` on success events. Emit which layer decided (0/1/2/3), the outcome, and the detail as FIELDS, not prose — machine-readable for 18d's reconciliation and greppable in `events.jsonl`. Follow the existing schema-v1 idiom in `events.rs`; do not invent a new store.
- **D-11 (policy locus): hardcoded table, no config knobs.** An exhaustive match in `devflow-core` so adding an outcome forces declaring its policy at compile time. Phase 16's D-03 admits `devflow.toml` knobs only where one is warranted; a safety-critical policy table is the opposite of a knob — a configurable fail-closed guarantee is not a guarantee.
- **D-12 (extraction, testability-driven):** the outcome→policy mapping lands in `devflow-core` as a PURE function taking typed outcomes and returning an action enum — no I/O, no `CliError`. This is where the policy belongs on its own merits and makes the fail-closed paths unit-testable without spawning an agent. It follows the 13-01 precedent (`prepare_loop_back_to_code` split out of `loop_back_to_code` for exactly this reason). Shrinking `advance()` is a side effect, not the goal. See Deferred for what is explicitly NOT extracted.

**17c — Preflight readiness**

- **D-13 (check split): generic core + optional adapter hook.** A generic preflight runs the universal checks; `AgentAdapter` gains a `preflight()` method with an empty default body, mirroring the existing `extra_env` default (`agents/mod.rs:39-41`). Adapters opt in only where they differ. This is the trait surface Phase 18's Hermes adapter implements — it must consume this model, not define a competing one.
- **D-14 (universal vs adapter):** RESOLVES retrospective decision-gate Q3.
  - **Universal (generic layer):** plan interactivity vs. execution mode; required security artifact present; external credential validity.
  - **Adapter hook:** reviewer receiver set non-empty.
- **D-15 (failure semantics): named preflight gate + notify.** Not a hard exit. Unattended runs are the design target — the notify hook exists precisely because the operator is not watching the terminal, so a hard exit to stdout is invisible to a cron-launched run. Consistent with the WR-11 never-silent idiom.
- **D-16 (timing): before every stage launch, scoped to that stage's requirements.** Directly fixes the observed miss — Ship's empty reviewer set and invalid GitHub auth surfaced only after Ship's work had run. A single up-front check cannot evaluate Ship-specific requirements hours ahead, nor catch credentials that expire mid-phase.

**17d — Build provenance**

- **D-17 (self-dogfood detection): workspace identity match.** The target project root contains the DevFlow workspace (a `Cargo.toml` declaring `devflow-cli`/`devflow-core`). Deterministic, offline, no config, no false positives on unrelated Rust projects. Rejected: git-remote match (breaks on forks, SSH-vs-HTTPS spellings, remote-less clones) and explicit opt-in (failure mode is forgetting to set it — which is how the incident happened).
- **D-18 (strictness): block self-dogfood on stale, warn elsewhere.** Strict is the default where it is cheap. Only affects DevFlow's own repo; ordinary projects running a released binary are untouched and need no source checkout. Justified by cost: the incident consumed an entire spike phase reading false evidence.
- **D-19 (staleness definition): composite.** Stale = embedded commit is not an ancestor of HEAD, **OR** source is newer than the build timestamp. Ancestry catches the exact incident (a Homebrew symlink to a release build predating the phase's fixes); the mtime arm catches an uncommitted working tree the binary predates, which ancestry alone misses. Rejected: bare `commit != HEAD` (fires on any normal in-development state — alarms you learn to ignore are worse than none) and mtime-only (fragile across checkout/clone/rebase).
- **D-20 (build metadata): hand-rolled `build.rs`, no new dependencies.** ~30 lines shelling to git, emitting `cargo:rustc-env` vars. No `build.rs` or version-embedding infrastructure exists today. Rejected `vergen` — a build dependency and its tree for what a few readable lines do. MUST degrade gracefully when git metadata is unavailable (crates.io installs have no `.git`) — absence of provenance is not staleness.
- **D-21 (event payload):** `workflow_started` currently carries only agent/mode/worktree (`main.rs:605-614`). Extend with version, commit, dirty flag, build timestamp, and resolved executable path. `std::env::current_exe()` is already used at `monitor.rs:79` — reuse that precedent.

### Claude's Discretion

- Exact typed-outcome variant names and the shape of the structured evidence record (D-10), within the schema-v1 convention.
- Separate-counter ceiling values and backoff curve for D-08/D-09.
- Whether the D-12 pure policy function lives in a new `devflow-core` module or an existing one.
- Preflight check implementation order and how stage-scoped requirements are declared (D-16).

### Deferred Ideas (OUT OF SCOPE)

- **Full `main.rs` orchestration extraction** — deliberately NOT done in Phase 17 (see 17-CONTEXT.md `<deferred>` for the full 4-point rationale: stale line-count premise, error-type redesign risk, risk asymmetry with the fail-closed paths this phase proves correct, and the narrower 13-01 precedent). If the cluster still warrants extraction after 17 ships, it earns its own phase.
- **Correct `CONCERNS.md`'s stale main.rs line count/framing** — small doc-accuracy fix, unchecked by the 16c doc-claim checker (operator-facing docs only, not `.planning/`).
- **18d — project-aware `devflow doctor` reconciliation** — moved to Phase 18. Depends on this phase's D-10 evidence records and D-21 provenance.
- **18e — WR-03 test stabilization** — moved to Phase 18. `parallel_creates_two_worktrees_and_spawns_two_monitors` (`crates/devflow-cli/tests/phase7_cli.rs:184-200`) races the monitor's capture archival.
- Hermes support (Phase 18) and full `main.rs` orchestration extraction: out of scope entirely.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| P1 (build provenance, from retrospective "Confirmed Finding") | Compile and expose build provenance (version, commit, build timestamp, executable path); emit in `workflow_started`; `devflow doctor`-style comparison against source checkout when self-dogfooding; strict mode blocks self-dogfood on stale build | See "Build Provenance (17d)" in Architecture Patterns; `build.rs` idiom in Code Examples; D-17–D-21 |
| P2 (completion + retry policy) | Typed agent outcomes (`rate_limited`, `resource_killed`, `agent_unavailable`, `unknown`, `failed`), deterministic resume/backoff guidance, explicit policy per outcome; `Unknown` must never auto-advance | See "Four-Layer Cascade" diagram, "Layer 2/3 Exit-Code Classification", Common Pitfall 1 (multi-word variant serialization), D-01–D-12 |
| P3 (preflight readiness) | Before launching a phase, scan for required interaction, reviewer availability, security artifacts, external credentials, declared post-condition probes; fail as a named preflight gate before agent time is consumed | See "Preflight Insertion Point" in Architecture Patterns, D-13–D-16 |
| P4 (state/event reconciliation) — **OUT OF SCOPE, deferred to 18d** | `devflow doctor`/`devflow recover --check` comparing state, events, branch ancestry | Not researched — explicitly deferred; do not plan for it |
| Acceptance criterion 1 | Failed Merge leaves branch intact, blocks terminal hooks, opens Ship gate | Already covered by a Phase 16 regression test — verify against final HEAD only, do not re-plan (see Environment/Validation notes) |
| Acceptance criterion 2 | `workflow_started` records executable/build provenance; self-dogfood detects and rejects a stale binary before stage launch | Same as P1 |
| Acceptance criterion 3 | `unknown` completion cannot reach next stage without explicit approval or a successful declared external post-condition | Same as P2, specifically D-01/D-02/D-03/D-05 |
| Acceptance criterion 4 | Non-interactive plan, unavailable reviewer, missing security artifact, or invalid required credential reported before stage launch | Same as P3 |
</phase_requirements>

## Summary

Phase 17 is a pure Rust-workspace hardening phase against a codebase whose shape is already well-precedented: a four-layer completion-decision cascade in `devflow-core::agent_result`, a stage-advance dispatcher in `devflow-cli::main::advance()`, an `AgentAdapter` trait with default-method extension points, a `devflow.toml`+env-var config precedent, and an append-only `events.jsonl` schema-v1 log. All four scope units (17a–17d) extend existing mechanisms rather than introducing new ones — there is no new external dependency anywhere in this phase (D-20 explicitly rejects `vergen`; the retry/outcome work stays inside `devflow-core`; the preflight hook mirrors the existing `extra_env` default-impl pattern).

The highest-risk area is 17a/17b: the four-layer cascade (`Layer 0` external-probe → `Layer 1` DEVFLOW_RESULT marker/envelope → `Layer 2` exit-code+commit-gate → `Layer 3` process-gone-with-commits) has genuinely different zero-commit semantics at each layer, and the decisions (D-01 through D-12) touch three of the four layers plus `advance()`'s dispatch. The planner must trace each layer independently per stage — `CONCERNS.md` already flags this cascade as fragile and under-tested at Layer 3, and this research confirms Layer 2's existing "stage NOT in {Plan,Code}, commits=0 → Success" comment is deliberate normal-operation behavior that must NOT be touched by D-03's "no declaration → ambiguous → fail" rule, which instead targets Layer 3's "process is gone" branch specifically (see Pitfall 2 below — this is the single easiest place to introduce a regression).

17c (preflight) and 17d (build provenance) are comparatively mechanical: 17c is a new `AgentAdapter::preflight()` default method plus a generic pre-launch check function called from `launch_stage()`, using the exact same gate+notify machinery (`gates::fire_gate_notify`) already wired for WR-11's never-silent failures. 17d is a ~30-line `build.rs` in `devflow-cli` (git shells out, `cargo:rustc-env`/`cargo:rerun-if-changed`) plus a runtime staleness check using `git merge-base --is-ancestor` and an mtime comparison, gated to fire only when the *target* project (not necessarily DevFlow's own build machine) is DevFlow's own workspace.

**Primary recommendation:** Implement 17d and 17c first (mechanical, low blast radius, no cascade interaction), then 17b's typed-outcome/policy split (D-07, D-11, D-12 as a pure function), then 17a last since it depends on 17b's taxonomy and touches the most fragile part of the codebase (Layer 0/2/3 + `advance()`'s dispatch) — this ordering also lets 17d's build-provenance event fields and 17b's structured evidence fields (D-10, D-21) land in the SAME `workflow_started`/`advance_evaluated` event-schema change if the planner chooses to batch them, since both touch `events::emit` call sites in `main.rs`.

## Architectural Responsibility Map

This is a single-binary CLI + library workspace (`devflow-core` library, `devflow-cli` binary), not a web app — tiers below are adapted accordingly.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Typed outcome classification (Layer 0-3 cascade) | `devflow-core` (library: `agent_result.rs`) | — | Pure evaluation logic against on-disk capture files + git; must stay side-effect-free and unit-testable without spawning agents (existing precedent: all four layers are already here) |
| Outcome → action policy (gate / retry / abort) | `devflow-core` (new pure function, D-12) | `devflow-cli` (`advance()` calls it) | D-11/D-12: policy is a pure, exhaustively-matched function in core; `main.rs` only orchestrates I/O (gate files, launches) around its output — mirrors the `prepare_loop_back_to_code`/`launch_stage` split from 13-01 |
| Rate-limit auto-resume (cron-instructions) | `devflow-core` (`ship.rs`: `build_cron_instructions`/`write_cron_instructions`) | `devflow-cli` (`main.rs`: currently only wired into `sequentagent`, not the primary `advance()` monitor loop) | Existing machinery lives in core; the gap is that the single-agent `advance()` path never calls it today — only `sequentagent` does (see Pitfall 3) |
| Preflight readiness checks (generic) | `devflow-cli` (`launch_stage()`, before agent spawn) | `devflow-core` (`AgentAdapter::preflight()` default method) | Needs stage/mode/state context that only the CLI orchestration layer has; adapter-specific checks are a trait hook so Phase 18's Hermes adapter can extend without touching the generic layer |
| Build provenance embedding | Build script (compile-time, `devflow-cli/build.rs`) | — | `cargo:rustc-env` only works at compile time in the binary crate that consumes it via `env!()` |
| Build staleness detection (runtime) | `devflow-cli` or `devflow-core` (git subprocess calls, same pattern as `evaluate_layer2`'s `git rev-list`) | — | Runs against the *target* project's git checkout at every stage launch (D-16 timing), not at DevFlow's own build time |
| Event schema extension (`workflow_started`, `advance_evaluated`) | `devflow-core` (`events::emit` — schema v1) | `devflow-cli` (call sites in `main.rs`) | D-10/D-21 both extend the JSON payload fields; the envelope keys (`v`,`ts`,`phase`,`event`) stay fixed per `events.rs`'s "envelope keys win" guarantee |
| Monitor-owned exit code capture | OS process layer (`sh -c ... wait $apid; echo $? > exit_file`, `monitor.rs`) | `devflow-core` (`evaluate_layer2` reads the plain-integer exit file) | Exit code 137 (SIGKILL) already reaches `evaluate_layer2` as a plain `i32` via shell `$?` — no Rust `ExitStatusExt::signal()` call is needed or possible here (see Common Pitfall 1a) |

## Standard Stack

### Core

No new runtime dependencies. This phase is entirely implemented with the workspace's existing dependency set.

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde` / `serde_json` | 1.x (workspace pin) | Structured evidence records (D-10), new `AgentStatus` variants | Already the workspace's exclusive (de)serialization stack; every existing typed status/verdict uses it |
| `thiserror` | 2.x (workspace pin) | Any new error variants (e.g. a preflight-check error type) | Already the workspace's exclusive error-derive stack (`ResultError`, `ShipError`, `GateError`, `MonitorError` all use it) |
| `libc` | 0.2 (devflow-core dep, existing) | Already used for `kill(pid, 0)` liveness checks in `agent.rs`; NOT needed for exit-code-137 detection since the shell already converts signal death to a plain integer (see Pitfall 1a) | — |

### Supporting

No new supporting libraries. Explicitly rejected candidates and why:

| Candidate | Verdict | Why Rejected |
|-----------|---------|--------------|
| `vergen` (or `vergen-git2`) | REJECTED (D-20 explicit) | A build dependency and its tree for what ~30 lines of hand-rolled `git` shelling does; the workspace has zero build-dependency precedent today |
| `chrono` / `time` | Not needed | `ship.rs` already hand-rolls RFC3339-ish timestamp parsing (`parse_rfc3339ish`, `civil_from_days`) without a date/time crate — follow this precedent for any new build-timestamp formatting rather than introducing a date crate |
| `git2` (libgit2 bindings) | Not needed | Every existing git interaction in this codebase (`evaluate_layer2`, `evaluate_layer3`, `GitFlow`) shells out to the `git` binary via `std::process::Command`; `build.rs` should do the same for consistency and because `git2` would be a build-time-only dependency addition D-20 explicitly rejects the class of |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled `build.rs` + `env!()` | `vergen` | Rejected per D-20 — see above |
| `git merge-base --is-ancestor` (shell) | `git2::Repository::merge_base` | Same rejection rationale — no new dependency, and the shell-out pattern is already this codebase's exclusive git-interaction idiom |

**Installation:** None — no new packages to install for this phase.

**Version verification:** N/A — no new packages. Workspace toolchain verified live: `cargo 1.97.1`, `rustc 1.97.1`, `git 2.55.0`, `gh 2.96.0` (all present in the execution environment; `gh` is relevant to D-14's "external credential validity" preflight check — `gh auth status` is a viable minimal-live-test probe for GitHub-auth validity, consistent with the project's minimal-live-tests idiom).

## Package Legitimacy Audit

**Not applicable — this phase installs no external packages.** D-20 explicitly locks the build-provenance implementation to a hand-rolled `build.rs` with zero new dependencies (`workspace.dependencies` in the root `Cargo.toml` is unchanged by this phase). No `[build-dependencies]` section exists or is needed in `crates/devflow-cli/Cargo.toml`.

**Packages removed due to [SLOP] verdict:** none (none proposed).
**Packages flagged as suspicious [SUS]:** none.

## Architecture Patterns

### System Architecture Diagram

```
                          devflow start --phase N --agent X --mode auto|supervise
                                          │
                                          ▼
                              main.rs: start() saves State, emits "workflow_started"
                              (◄── D-21 extends this payload with version/commit/
                                   dirty/build-timestamp/exe-path)
                                          │
                                          ▼
                              launch_stage(state, ...)
                                 ├─► [NEW 17c] run_preflight(state, adapter)
                                 │      ├─ generic checks (D-14 universal):
                                 │      │    - plan interactivity vs execution mode
                                 │      │    - required security artifact present
                                 │      │    - external credential validity (e.g. `gh auth status`)
                                 │      ├─ adapter.preflight() (D-14 adapter-specific):
                                 │      │    - reviewer receiver set non-empty
                                 │      └─ on failure (D-15): named preflight gate +
                                 │           gates::fire_gate_notify() — NOT a hard exit
                                 ├─► [NEW 17d] check_build_staleness(target_project)
                                 │      ├─ D-17: is target_project the DevFlow workspace itself?
                                 │      ├─ D-19: embedded_commit ancestor-of HEAD?
                                 │      │        OR source mtime > build timestamp?
                                 │      └─ D-18: strict→block if self-dogfood+stale;
                                 │                warn otherwise
                                 ├─► ensure_agent_binary(program)   (existing, unchanged)
                                 ├─► agent_result::archive_phase_files(...)  (existing)
                                 └─► monitor::spawn_monitor(state, program, args, envs)
                                          │
                                          ▼
                     [detached sh process] launches agent, captures stdout/stderr/exit,
                     then invokes `devflow advance --phase N` on agent exit
                                          │
                                          ▼
                              advance(project_root, phase)
                                 └─► agent_result::evaluate_agent_result(...)
                                        │
                                        ▼
                         ┌─────────────────────────────────────────────┐
                         │  FOUR-LAYER CASCADE (agent_result.rs)        │
                         │                                               │
                         │  Layer 0: external post-condition probe      │
                         │   ├─ [EXTENDED D-05] any stage now eligible   │
                         │   │    (was Code-only)                       │
                         │   ├─ failing probe → Failed (unchanged,      │
                         │   │    authoritative)                        │
                         │   └─ [NEW D-05] ALL declared probes pass →   │
                         │        Success (was: defer to Layer 1/2/3)   │
                         │        ↓ if None (no declaration) ↓          │
                         │  Layer 1: DEVFLOW_RESULT marker / envelope   │
                         │   (unchanged — Success/Failed/RateLimited)   │
                         │        ↓ if None ↓                           │
                         │  Layer 2: exit code + commit-count gate      │
                         │   ├─ exit≠0 → Failed (ALL stages, unchanged) │
                         │   ├─ [NEW D-07] exit==137 → ResourceKilled   │
                         │   ├─ [NEW D-07] exit==127 → AgentUnavailable │
                         │   ├─ exit=0, Plan|Code, commits=0 → Failed   │
                         │   │    (unchanged — "no work done")          │
                         │   └─ exit=0, Define|Validate|Ship, commits=0 │
                         │        → Success (UNCHANGED — legitimate;    │
                         │        do NOT fold this into D-03's case 3)  │
                         │        ↓ if exit file missing ↓              │
                         │  Layer 3: process gone, commits inspected    │
                         │   ├─ [D-02] SPLIT into typed outcomes        │
                         │   │    instead of one blanket Unknown        │
                         │   ├─ commits > 0 → typed "unverified" outcome│
                         │   │    (still requires D-06 non-advance gate)│
                         │   └─ [D-03 case 3 applies HERE] commits == 0,│
                         │        no Layer-0 declaration → Failed,      │
                         │        notify human (this is THE ambiguous  │
                         │        case D-03 describes — "process gone, │
                         │        nothing accounted for")               │
                         └─────────────────────────────────────────────┘
                                        │
                                        ▼
                         [NEW 17b] pure policy fn(stage, outcome) -> Action
                            (D-11 exhaustive match, D-12 pure/no I/O, lives in
                             devflow-core; D-08 infra outcomes use a SEPARATE
                             counter, never consecutive_failures)
                                        │
                                        ▼
                         advance()'s stage dispatch (main.rs:854-887)
                            [D-01/D-06 FIX] every stage's non-Success/non-declared-
                            pass outcome routes to gate/loop/abort — Unknown (or
                            its typed replacement) NEVER silently advances
                                        │
                              ┌─────────┴─────────┐
                              ▼                   ▼
                    Action::AutoResume        Action::Gate / Retry / Abort
                    [NEW D-09, RateLimited     (existing handle_stage_failure /
                     only] — reuse ship.rs's   handle_validate_outcome /
                     build_cron_instructions +  handle_ship_failure / run_gate,
                     write_cron_instructions,   extended with D-10 structured
                     currently only wired into  evidence in "advance_evaluated"
                     sequentagent, NOT the      event fields
                     primary advance() path
                     (see Pitfall 3)
```

### Recommended Project Structure

No new files/directories are architecturally required beyond:

```
crates/devflow-cli/
├── build.rs                    # NEW (17d) — emits cargo:rustc-env vars for
│                                #   version/commit/dirty/build-timestamp
├── src/main.rs                 # advance()/launch_stage() edits (17a/17b/17c/17d)
crates/devflow-core/
├── src/agent_result.rs         # AgentStatus new variants (17b), Layer 0/2/3
│                                #   edits (17a)
├── src/agents/mod.rs            # AgentAdapter::preflight() default method (17c)
├── src/events.rs                # no structural change — D-10/D-21 only add
│                                #   payload fields at call sites
├── src/ship.rs                  # no structural change — D-09 reuses
│                                #   build_cron_instructions/write_cron_instructions
├── src/                         # [Claude's discretion, D-12] new module for the
│   └── outcome_policy.rs?       #   pure outcome→action function, OR fold into
│                                #   agent_result.rs — planner decides
```

### Pattern 1: Default-method trait extension for opt-in adapter behavior (D-13)

**What:** Add a method to `AgentAdapter` with an empty default body; only adapters that need different behavior override it.
**When to use:** Any per-adapter hook where most adapters share identical (no-op) behavior — exactly `extra_env`'s existing shape.
**Example:**
```rust
// Source: crates/devflow-core/src/agents/mod.rs:39-41 (existing precedent, extend analogously)
pub trait AgentAdapter {
    // ... existing methods ...

    /// Extra environment variables for the agent process tree.
    fn extra_env(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    // NEW (D-13): adapter-specific preflight checks (D-14: reviewer receiver
    // set non-empty is the only adapter-specific universal check identified
    // so far — most adapters will use this default).
    fn preflight(&self, state: &crate::state::State) -> Result<(), PreflightError> {
        Ok(())
    }
}
```

### Pattern 2: Pure policy function separated from I/O orchestration (D-12)

**What:** An outcome→action mapping with no side effects, tested directly with constructed inputs — no temp dirs, no spawned processes.
**When to use:** Any decision logic that must be exhaustively correct (fail-closed) and is currently entangled with `CliError`/filesystem calls in `main.rs`.
**Example:**
```rust
// Source: crates/devflow-cli/src/main.rs:1164-1192 (prepare_loop_back_to_code,
// the established 13-01 precedent this pattern follows)
// New analogous shape for D-12 (illustrative — exact types are Claude's discretion):
pub fn decide_action(stage: Stage, outcome: AgentStatus, declared_probe: bool) -> Action {
    match (stage, outcome) {
        // exhaustive match forces every future AgentStatus variant to be
        // handled here — the type system prevents the D-01 regression class
        (_, AgentStatus::Success) => Action::Advance,
        (_, AgentStatus::RateLimited) => Action::AutoResume, // D-09
        (_, AgentStatus::ResourceKilled) => Action::GateInfra, // D-08 separate counter
        (_, AgentStatus::AgentUnavailable) => Action::GateInfra,
        (_, AgentStatus::Failed) => Action::GateOrAbort,
        (_, AgentStatus::Unknown) => Action::GateOrAbort, // never Advance (D-01/D-06)
    }
}
```

### Anti-Patterns to Avoid

- **Reusing `Debug`-derived lowercasing for multi-word enum variants in wire/event output:** `format!("{:?}", status).to_ascii_lowercase()` (used today at `main.rs:848` for `advance_evaluated`'s `status` field) collapses `RateLimited` to `"ratelimited"`, not `"rate_limited"`. The same collapse happens with `#[serde(rename_all = "lowercase")]` (used on `AgentStatus` today) — it lowercases but does NOT insert word separators. Both new D-07 variants (`ResourceKilled`, `AgentUnavailable`) will silently collapse to `"resourcekilled"`/`"agentunavailable"` under either existing mechanism unless the planner adds explicit `#[serde(rename = "...")]` per multi-word variant AND a dedicated (not `Debug`-based) formatter for event emission. See Common Pitfall 1 for full detail — this is VERIFIED against the current source, not assumed.
- **Folding D-03's zero-commit ambiguity into Layer 2's Define/Validate/Ship branch:** Layer 2's existing "exit=0, non-Plan/Code-stage, commits=0 → Success" behavior is intentional (the code comment says so explicitly) and must NOT change — the ambiguous case D-03 targets is Layer 3's "process is gone entirely, no exit code was even recorded" branch. See Pitfall 2.
- **Wiring D-09's auto-resume into `sequentagent` only:** the cron-instructions machinery today (`write_rate_limit_cron`, `build_cron_instructions`) is called exclusively from the two-agent `sequentagent` handoff path — the single-agent monitor-driven `advance()` loop (the PRIMARY dogfood path, and the one the retrospective's findings came from) never calls it. D-09 requires wiring this into `advance()`'s RateLimited handling too. See Pitfall 3.
- **Using `std::os::unix::process::ExitStatusExt::signal()` to detect SIGKILL:** the monitor never gives DevFlow a Rust `ExitStatus` — it captures the exit code via a POSIX shell (`wait $apid; echo $? > exit_file`), which already encodes signal death as `128+signal` (137 for SIGKILL) as a plain text integer. `evaluate_layer2` just needs `exit_code == 137`, not any signal-extension API. See Pitfall 1a.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Git commit hash / dirty-tree detection at build time | A custom `.git` file parser | `git rev-parse HEAD`, `git status --porcelain` shelled from `build.rs` (matches every other git interaction in this codebase) | The codebase's own precedent (`evaluate_layer2`, `GitFlow`) already shells to `git`; parsing `.git/HEAD`/packed-refs by hand is exactly the kind of edge-case-laden reimplementation this codebase avoids elsewhere |
| Ancestor-of-HEAD staleness check | Manual commit-graph walk | `git merge-base --is-ancestor <embedded_commit> HEAD` (exit 0 = ancestor, exit 1 = not, other = error — e.g. commit unknown to a shallow clone) | Exact-purpose, well-defined exit-code contract; hand-rolling a graph walk in Rust duplicates git's own logic and will miss edge cases (shallow clones, unreachable commits) |
| Retry/backoff scheduling for rate limits | A new scheduler | Extend the existing `cron-instructions-NN.json` + Hermes-cron machinery in `ship.rs` (`build_cron_instructions`/`cron_schedule_from_retry_after`) | D-09 explicitly says this; the machinery already parses retry timestamps into cron schedules and is per-phase-file-safe under `devflow parallel` |
| Structured event logging | A new log format/store | `events::emit()` — schema v1, already fail-soft, already handles concurrent-phase interleaving via `O_APPEND` | D-10 explicitly says "do not invent a new store" |

**Key insight:** Every mechanism this phase needs already exists in skeletal or adjacent form somewhere in the codebase — the work is almost entirely *extension* (lifting a stage restriction, adding enum variants, adding a trait default method, adding event-payload fields) rather than *invention*. The one genuinely new artifact is `build.rs`, which is itself the textbook-standard shape for this exact problem in the Rust ecosystem (confirmed against `rust-lang/cargo`'s own `build.rs` and the general `cargo:rustc-env` + `cargo:rerun-if-changed=.git/HEAD` pattern).

## Common Pitfalls

### Pitfall 1a: Exit code 137 is already a plain integer by the time Rust sees it — don't look for `ExitStatusExt::signal()`

**What goes wrong:** A planner unfamiliar with `monitor.rs`'s design might add a call to `std::os::unix::process::ExitStatusExt::signal()` somewhere, expecting to intercept a `std::process::ExitStatus` for the agent process directly in Rust.
**Why it happens:** The natural Rust idiom for signal detection is `ExitStatusExt` — but DevFlow's monitor is a detached POSIX shell script (`monitor.rs:148-160`), not a Rust `Command::status()` call on the agent itself. The shell backgrounds the agent (`"$@" > stdout 2>stderr & apid=$!`), then does `wait $apid; echo $? > exit_file`. POSIX shell's `$?` after `wait` on a signal-killed child is `128+signal` (137 for SIGKILL=9) as an ordinary decimal string — this is shell/POSIX behavior, not something Rust's process APIs touch at all. `evaluate_layer2` (`agent_result.rs:521-586`) already reads this integer from the exit file via `.trim().parse::<i32>()`.
**How to avoid:** Add the `exit_code == 137` (and `== 127` for `AgentUnavailable`, i.e. shell's "command not found") branches directly inside `evaluate_layer2`'s existing match on `exit_code`, using the same plain-`i32` value already being read. No new process-spawning or signal-extension code is needed anywhere in this phase.
**Warning signs:** Any diff that imports `std::os::unix::process::ExitStatusExt` or calls `.signal()` on anything in this phase is very likely solving the wrong layer of the problem.

### Pitfall 2: D-03's "no declaration → ambiguous → fail" rule targets Layer 3, not Layer 2's existing Define/Validate/Ship zero-commit Success path

**What goes wrong:** Reading D-03 in isolation ("zero commits + no declaration at all → treat as failure") could be misapplied to Layer 2's documented decision matrix (`agent_result.rs:507-513`), which currently returns `Success` for `exit=0, stage NOT in {Plan, Code}, commits=0` — e.g. a clean Define-stage exit with zero commits (completely normal; Define does discovery, not code changes).
**Why it happens:** Both cases are "zero commits, non-Code-stage" on the surface. But they are distinguished by whether the agent's exit was OBSERVED (Layer 2: exit file exists, exit code recorded, the agent told us something) vs. UNOBSERVED (Layer 3: process is simply gone, no exit code was ever written — e.g. monitor or agent crashed before writing `phase-NN-exit`). D-01's own bug report is specific: `evaluate_layer3` (`agent_result.rs:610-620`), not `evaluate_layer2`, is the origin of the false-Unknown-advances-silently defect.
**How to avoid:** Confirm with a direct read of `evaluate_layer2`'s decision-matrix doc comment (lines 507-513) before touching it. D-03's three-way policy is realized by (a) extending Layer 0 to affirmatively return Success/Failed based on declared probes (handles cases 1 and 2), and (b) splitting Layer 3's blanket `Unknown` into a typed outcome that, for the zero-commit sub-case, becomes a failure requiring human notification (handles case 3) — per D-02's "split Layer 3" fix locus. Layer 2's Define/Validate/Ship zero-commit Success branch is untouched by D-03.
**Warning signs:** A diff to `evaluate_layer2`'s `commit_gated`/`no_work_done` logic in this phase is a strong signal the ambiguity above wasn't traced — `CONCERNS.md` explicitly flags this cascade as fragile with under-tested Layer 3, and the canonical_refs instruct tracing ALL layers for ALL stage types before editing.

### Pitfall 3: D-09's rate-limit auto-resume machinery exists but is wired only into `sequentagent`, not the primary `advance()` loop

**What goes wrong:** Assuming `RateLimited` already auto-resumes because `build_cron_instructions`/`write_cron_instructions`/`cron_schedule_from_retry_after` exist in `ship.rs` and are exercised by tests (`phase7_cli.rs:444`).
**Why it happens:** Those functions ARE fully built and tested — but every call site (`main.rs:1670-1685`, `write_rate_limit_cron` at `main.rs:1735`) is inside `sequentagent()`, the two-agent rebase-handoff command. The single-agent, monitor-driven `advance()` path (`main.rs:788-887`) — which is what every ordinary `devflow start` run uses, and what the Phase 16 dogfood findings came from — currently treats `RateLimited` identically to `Failed` (`main.rs:854-857`: `matches!(result.status, AgentStatus::Failed | AgentStatus::RateLimited)`), routing it through the same never-silent gate as a hard failure. This confirms the retrospective's finding verbatim: "Rate limits ... repeatedly looped back to Code and opened gates without an actionable recovery classification."
**How to avoid:** D-09's fix must add a NEW call path from `advance()`'s RateLimited branch into `write_rate_limit_cron`/`build_cron_instructions` (or equivalent), NOT assume the existing `sequentagent` wiring already covers it. The two entry points (`advance()` vs `sequentagent()`) will likely need a small shared helper to avoid duplicating the cron-instructions construction logic.
**Warning signs:** If the phase's diff to `main.rs` only touches functions inside the `// parallel / sequentagent` section (search for that comment, `main.rs:1384`), D-09 has not actually been implemented for the path the retrospective evidence came from.

### Pitfall 4: `git merge-base --is-ancestor` errors (not just "false") when the embedded commit is unknown to the local repo

**What goes wrong:** Treating any non-zero exit from `git merge-base --is-ancestor <embedded> HEAD` as "stale."
**Why it happens:** Per git's documented contract [CITED: git-scm.com/docs/git-merge-base], exit 0 = ancestor (true), exit 1 = not an ancestor (false), and any OTHER exit code signals an error — e.g. the embedded commit hash is simply not known to this checkout at all (a shallow clone, a commit that was later garbage-collected, or — relevant here — a binary built from a DIFFERENT repository's history entirely, such as a fork). Collapsing "definitely stale" and "cannot determine" into the same UI/blocking behavior would misclassify the latter.
**How to avoid:** Distinguish the exit-1 case (definitively not an ancestor → stale, per D-19) from any other non-zero exit (error/indeterminate → warn, don't hard-block, and surface the git error text) — this also interacts with D-20's "must degrade gracefully when git metadata is unavailable" requirement (e.g. `.git` entirely absent for a crates.io install has no embedded-commit ancestry question to ask at all).
**Warning signs:** A staleness check that treats "cannot run git merge-base" (e.g. `git` not on PATH, embedded commit absent) the same as "confirmed non-ancestor" will produce false self-dogfood-stale alarms in legitimate scenarios (e.g. a shallow CI checkout), which D-19's own rationale explicitly says to avoid ("alarms you learn to ignore are worse than none").

## Code Examples

### `build.rs` idiom for embedding git provenance (D-20)

```rust
// Source: general cargo build-script convention (cargo:rustc-env +
// cargo:rerun-if-changed=.git/HEAD), cross-checked against
// https://doc.rust-lang.org/cargo/reference/build-scripts.html and
// https://github.com/rust-lang/cargo/blob/master/build.rs for the
// rerun-if-changed idiom. [CITED — no exact equivalent exists in this
// codebase yet since no build.rs currently exists anywhere in the workspace.]
use std::process::Command;

fn main() {
    // Re-run only when git refs actually move — not on every `cargo build`.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    let commit = run_git(&["rev-parse", "HEAD"]);
    let dirty = run_git(&["status", "--porcelain"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    // D-20: MUST degrade gracefully when git metadata is unavailable
    // (crates.io installs have no .git) — absence of provenance is not
    // staleness. Emit an empty/sentinel value rather than failing the build.
    println!(
        "cargo:rustc-env=DEVFLOW_BUILD_COMMIT={}",
        commit.unwrap_or_default()
    );
    println!("cargo:rustc-env=DEVFLOW_BUILD_DIRTY={dirty}");
    println!(
        "cargo:rustc-env=DEVFLOW_BUILD_TIMESTAMP={}",
        // Hand-rolled, matching ship.rs's existing no-chrono-dependency
        // precedent — a plain Unix-seconds integer is sufficient; the
        // runtime staleness check only needs to compare it to file mtimes.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
}

fn run_git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
```

**Modern cargo syntax note:** cargo 1.77+ (this workspace runs 1.97.1) supports the double-colon form `cargo::rustc-env=...`/`cargo::rerun-if-changed=...` for built-in instructions, disambiguating them from custom `cargo:key=value` metadata a dependent crate might read. Either form works on this toolchain; the single-colon form above matches what most existing examples in the wild still use. [CITED: WebSearch of cargo build-script conventions; exact form is a minor style choice, not a locked decision]

### Ancestor-of-HEAD staleness check (D-19)

```rust
// Source: git-merge-base(1) documented exit-code contract
// https://git-scm.com/docs/git-merge-base — exit 0 = is-ancestor (true),
// exit 1 = is-not-ancestor (false), other = error (e.g. unknown commit).
// Follows this codebase's existing git-shelling pattern from
// crates/devflow-core/src/agent_result.rs's evaluate_layer2/evaluate_layer3.
fn embedded_commit_is_stale(project_root: &std::path::Path, embedded_commit: &str) -> Staleness {
    if embedded_commit.is_empty() {
        return Staleness::Unknown; // D-20: no provenance != staleness
    }
    let output = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", embedded_commit, "HEAD"])
        .current_dir(project_root)
        .output();
    match output.map(|o| o.status.code()) {
        Ok(Some(0)) => Staleness::Fresh,   // embedded commit IS an ancestor
        Ok(Some(1)) => Staleness::Stale,   // definitively NOT an ancestor (Pitfall 4)
        _ => Staleness::Indeterminate,      // error / commit unknown — warn, don't block
    }
}
```

### Reusing the existing default-method + notify pattern for preflight (D-13/D-15)

```rust
// Source: crates/devflow-core/src/gates.rs:282 (fire_gate_notify — existing,
// reused verbatim, not reinvented) + crates/devflow-cli/src/main.rs:991
// (run_gate call shape from handle_stage_failure, the WR-11 never-silent
// precedent D-15 explicitly follows)
fn run_preflight(project_root: &Path, state: &mut State, adapter: &dyn AgentAdapter) -> Result<(), CliError> {
    if let Err(reason) = generic_preflight_checks(state) // D-14 universal checks
        .and_then(|()| adapter.preflight(state))          // D-14 adapter hook
    {
        // D-15: named preflight gate + notify, NOT a hard exit — the same
        // gate+notify machinery WR-11 already established for never-silent
        // failures, applied one stage earlier (before the agent even spawns).
        return match run_gate(project_root, state, state.stage, &format!("preflight failed: {reason}"))? {
            GateAction::Advance => launch_stage(state, None, None), // retry after operator fixes it
            GateAction::LoopBack(_) => launch_stage(state, None, None),
            GateAction::Abort(reason) => abort(project_root, state, &reason),
        };
    }
    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `evaluate_layer0` returns only `None`/`Some(Failed)`, Code-stage-only | (This phase, D-05) extends to `Some(Success)` on all-pass, every stage | Phase 17 | Enables legitimate external-only stages (any stage, not just Code) to complete cleanly with zero commits |
| `AgentStatus` has 4 variants (`Success`, `Failed`, `RateLimited`, `Unknown`) | (This phase, D-07) adds `ResourceKilled`, `AgentUnavailable` | Phase 17 | Retrospective's proposed 5-outcome taxonomy becomes fully typed |
| `RateLimited` handled identically to `Failed` in `advance()`'s primary loop | (This phase, D-09) `RateLimited` auto-resumes via cron-instructions in the primary `advance()` path, not just `sequentagent` | Phase 17 | Closes the exact gap the retrospective observed live |
| `workflow_started` payload: agent/mode/worktree only | (This phase, D-21) adds version/commit/dirty/build-timestamp/exe-path | Phase 17 | Makes a stale-binary dogfood run detectable after the fact, closing the Phase 16 incident's root cause |
| No `build.rs` anywhere in the workspace | (This phase, D-20) `devflow-cli/build.rs` hand-rolled, no new deps | Phase 17 | First build-script in the workspace's history |

**Deprecated/outdated:** Nothing in this phase deprecates prior Phase 16 mechanisms — Layer 0's `TRUST_EXTERNAL_VERIFY_ENV` approval mechanism is explicitly preserved unchanged (D-05), and `devflow.toml`'s existing knobs (`capture_retention`, `review_angles`, `external_verify_enabled`) are untouched (D-11 deliberately keeps the new outcome-policy table OUT of `devflow.toml`).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The D-12 pure policy function's exact module location (new file vs. folded into `agent_result.rs`) — presented as illustrative in Code Examples | Architecture Patterns / Pattern 2 | Low — explicitly Claude's Discretion per CONTEXT.md; any reasonable placement satisfies D-12's testability requirement |
| A2 | Exact `#[serde(rename = "...")]` wire names for `ResourceKilled`/`AgentUnavailable` (`"resource_killed"`/`"agent_unavailable"` used illustratively, matching the retrospective's proposed names) | Common Pitfall 1 / Pattern 2 | Low — CONTEXT.md leaves "exact typed-outcome variant names" to Claude's Discretion; the pitfall (word-boundary collapse) is verified regardless of final naming choice |
| A3 | Separate infra-failure counter ceiling value and backoff curve (D-08/D-09) — no concrete numbers proposed in this research | Architecture Patterns | Low — explicitly Claude's Discretion; the `MAX_CONSECUTIVE_FAILURES = 3` constant in `mode.rs` is cited only as a naming/placement precedent, not as the value to reuse |
| A4 | Modern cargo `cargo::` double-colon instruction syntax works identically to single-colon `cargo:` on this workspace's toolchain (1.97.1) | Code Examples | Low — cosmetic; either form compiles; verified cargo version supports both, but exact minimum-version cutover was not independently re-verified beyond the WebSearch summary |

**If this table is empty:** N/A — see entries above. All are LOW risk; none block planning, and all are already scoped as Claude's Discretion in the locked CONTEXT.md.

## Open Questions

1. **Where does the "external credential validity" universal preflight check (D-14) live, and which credential does it check first?**
   - What we know: `gh` CLI (2.96.0) is present in this dev environment; `gh auth status` is the obvious minimal-live-test probe for GitHub auth, consistent with the project's `feedback-minimal-live-tests` idiom. Ship is the stage where the retrospective observed invalid GitHub auth surfacing late.
   - What's unclear: Whether "external credential validity" as a UNIVERSAL (stage-agnostic) check means DevFlow probes `gh auth status` unconditionally for every stage, or only when the stage's hooks are known to need it (Ship's Merge/VersionBump/BranchCleanup hooks push to a remote — Define/Plan/Code do not). D-16 says checks are "scoped to that stage's requirements," which suggests the latter, but D-14 lists it under "Universal (generic layer)" rather than the adapter hook.
   - Recommendation: Planner should design the generic preflight as a set of independently-toggleable checks parameterized by stage (e.g. only run the `gh auth status` probe when the target stage's hook batch includes a remote-pushing hook), rather than one monolithic "run everything always" function — this satisfies both D-14 (universal = lives in the generic layer, not an adapter) and D-16 (scoped to stage's actual requirements).

2. **Does D-19's mtime-based staleness arm compare against the WORKING TREE's newest file mtime, or the latest commit's timestamp?**
   - What we know: D-19 says "source is newer than the build timestamp" and explicitly targets "an uncommitted working tree the binary predates, which ancestry alone misses."
   - What's unclear: A literal newest-file-mtime-in-worktree scan is expensive and noisy (touches from `cargo build` itself, editor saves, `.gitignore`d build artifacts) unless carefully scoped to tracked source files only (e.g. `git diff --stat` against the embedded commit, or `git status --porcelain` non-empty + comparing against build timestamp only when dirty).
   - Recommendation: Scope the mtime check to `git ls-files -m` (modified tracked files) mtimes rather than a full directory walk, and only evaluate it when `git status --porcelain` shows the tree is dirty (an ancestor-of-HEAD-but-clean tree needs no mtime check at all — HEAD's commit timestamp already covers that case via the ancestry arm).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | Layer 2/3 commit checks (existing), D-19 ancestor/staleness check (new), `build.rs` (new) | ✓ | 2.55.0 | None needed — already a hard requirement of every prior phase |
| `cargo`/`rustc` | Workspace build, `build.rs` execution | ✓ | 1.97.1 / 1.97.1 | None needed |
| `gh` (GitHub CLI) | D-14 external credential validity preflight check | ✓ | 2.96.0 | If absent on an operator's machine: the preflight check itself must fail-soft to a WARN (not a hard block) per the project's `external_verify_enabled`-style fail-soft precedent, since `gh` is not currently a hard dependency anywhere in this codebase |
| `sh` (POSIX shell) | Monitor's exit-code capture (existing, unchanged), any preflight probe shelling | ✓ (implicit — already required by `monitor.rs`) | — | None needed |

**Missing dependencies with no fallback:** none — every tool this phase touches is already present and already a soft/hard dependency of the existing codebase.

**Missing dependencies with fallback:** `gh` — see above; the preflight check design must not hard-fail the whole pipeline if `gh` itself is unavailable on the operator's machine (distinct from `gh auth status` reporting the user is unauthenticated, which SHOULD fail preflight).

## Validation Architecture

`workflow.nyquist_validation` is `true` in `.planning/config.json` — this section is required.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in Rust test harness — no external test framework) |
| Config file | none — tests are `#[cfg(test)]` modules inline (e.g. `agent_result.rs:971`, `mode.rs:82`, `main.rs`) plus integration tests under `crates/devflow-core/tests/` and `crates/devflow-cli/tests/` |
| Quick run command | `cargo test -p devflow-core agent_result::` (scope to the module under active edit) or `cargo test -p devflow-core` / `cargo test -p devflow-cli` per-crate |
| Full suite command | `cargo test` (workspace-wide; CI runs this exact command per `.github/workflows/ci.yml`) |

CI (`.github/workflows/ci.yml`) additionally runs `cargo clippy -- -D warnings` and `cargo fmt --check` as separate required jobs — both should be run locally before each commit per the workspace's existing convention (visible throughout prior phase SUMMARY.md entries).

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| P2 / AC-3 (17a: Unknown never auto-advances) | A `Success`-shaped `Layer 3` fallthrough (zero commits, no declaration) does NOT reach `Stage::Code => transition(..., Stage::Validate)` | unit | `cargo test -p devflow-core evaluate_layer3` / `cargo test -p devflow-cli advance` | ❌ Wave 0 — new test needed asserting the fixed Layer 3 typed outcome routes to gate, not transition |
| P2 (17a: D-05 Layer 0 extension) | A declared, approved, all-passing external probe with zero commits on a non-Code stage (e.g. Define) advances | unit | `cargo test -p devflow-core evaluate_layer0` | ❌ Wave 0 — existing tests only cover Code-stage Layer 0 |
| P2 (17b: D-07 new outcomes) | Exit code 137 → `ResourceKilled`; exit code 127 → `AgentUnavailable` | unit | `cargo test -p devflow-core evaluate_layer2` | ❌ Wave 0 |
| P2 (17b: D-08 separate counter) | A `ResourceKilled`/`AgentUnavailable` outcome does NOT increment `consecutive_failures` | unit | `cargo test -p devflow-cli` (new test near `main.rs:3291`'s existing consecutive-failures fixtures) | ❌ Wave 0 |
| P2 (17b: D-09 auto-resume) | A `RateLimited` outcome in the PRIMARY `advance()` path (not `sequentagent`) writes cron-instructions and does not fire a blocking gate | integration | `cargo test -p devflow-cli` (extend `phase7_cli.rs`'s existing `sequentagent_hands_off_after_rate_limit_and_writes_cron_instructions` pattern for the primary-loop case) | ❌ Wave 0 — existing test at `phase7_cli.rs:444` only covers `sequentagent` |
| P3 / AC-4 (17c: preflight) | Missing security artifact / invalid credential / empty reviewer set is reported via a named gate BEFORE `monitor::spawn_monitor` is called | unit/integration | `cargo test -p devflow-cli` (new test asserting `spawn_monitor` is never invoked when a preflight check fails — mirrors the existing `ensure_agent_binary` preflight test pattern at `main.rs:680-687`) | ❌ Wave 0 |
| P1 / AC-2 (17d: build provenance) | `workflow_started` event contains version/commit/dirty/build-timestamp/exe-path fields | integration | `cargo test -p devflow-cli` (extend event-payload assertion pattern from `events.rs`'s `emit_appends_parseable_lines_with_envelope_fields`) | ❌ Wave 0 |
| P1 / AC-2 (17d: self-dogfood staleness) | A target project matching the DevFlow workspace identity (D-17) with a non-ancestor embedded commit blocks stage launch | unit | `cargo test -p devflow-core` or `-p devflow-cli` (new — needs a fixture repo with two divergent commits, following the existing `init_repo_with_feature_commit` git-fixture pattern in `agent_result.rs:1001`) | ❌ Wave 0 |
| AC-1 (Merge-failure terminal contract) | Failed Merge leaves branch intact, blocks VersionBump/BranchCleanup, opens Ship gate | regression (existing) | `cargo test -p devflow-cli` — **already exists from Phase 16**; verify it still passes against final HEAD, do NOT re-plan or duplicate it | ✅ (Phase 16 regression test — locate via `rg -n "terminal_batch" crates/devflow-cli` or search `hooks_after_ship` test usages) |

### Sampling Rate
- **Per task commit:** scoped `cargo test -p <crate> <module>::` for the module just edited, plus `cargo clippy -- -D warnings` (CI-enforced)
- **Per wave merge:** full `cargo test` (workspace-wide)
- **Phase gate:** full suite green (`cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`) before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] New `evaluate_layer0` tests: non-Code stage + declared/approved/all-passing probe → `Success` (D-05)
- [ ] New `evaluate_layer2` tests: exit code 137 → `ResourceKilled`, exit code 127 → `AgentUnavailable` (D-07)
- [ ] New `evaluate_layer3` (or its typed replacement) tests: zero-commit/no-declaration → failure outcome, not blanket `Unknown` (D-01/D-02/D-03 case 3)
- [ ] New `advance()`-level test: `RateLimited` in the primary monitor loop writes cron-instructions (D-09) — extends the `sequentagent`-only pattern already in `phase7_cli.rs:444`
- [ ] New separate-counter test: infra outcomes never touch `consecutive_failures` (D-08)
- [ ] New preflight tests: each of D-14's universal checks + the adapter `preflight()` default-method override path (D-13)
- [ ] New `build.rs`/provenance tests: `workflow_started` payload fields (D-21), staleness detection with a two-commit git fixture (D-19), self-dogfood workspace-identity detection (D-17)
- [ ] No new test framework/config needed — `cargo test` + inline `#[cfg(test)]` modules cover the phase; only new test CASES are required, not new infrastructure

## Security Domain

`security_enforcement` is not present in `.planning/config.json` — treated as enabled per the default rule.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | This phase does not add a new authentication surface; it CHECKS existing credential validity (`gh auth status`) as a preflight probe, it does not manage credentials itself |
| V3 Session Management | No | N/A — no session concept in this CLI |
| V4 Access Control | No | N/A — single-operator local CLI, no multi-tenant access control surface |
| V5 Input Validation | Yes | The new `AgentStatus` variants and structured evidence fields (D-10) are deserialized from agent-controlled/parsed text (stdout, exit codes) — MUST follow the existing fail-safe deserialization idiom (`deserialize_verdict_lenient`'s pattern: malformed/unknown input becomes `None`/a safe default, never a parse error that could crash `advance()` or silently drop a valid signal) |
| V6 Cryptography | No | N/A — no new cryptographic operations in this phase |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Agent-controlled `reason`/`summary` text reaching a gate notification verbatim (shell injection into a notify command, or a multi-KB payload flooding a desktop notification) | Tampering / Denial of Service | ALREADY MITIGATED in this codebase: `truncate_reason`/`render_gate_context` (`main.rs:953-978`) strip control characters and cap length before any agent-derived text reaches a gate context; `gates::run_notify_command` passes gate metadata via environment variables, never string-interpolated into the notify command. Any NEW text surfaced by this phase (D-10's structured evidence, D-19's build-staleness reason strings) MUST route through the same truncation/sanitization before reaching `run_gate`'s `context` parameter or `events::emit` |
| Shell command injection via `build.rs`'s `git` subprocess calls | Tampering | `build.rs` should use `Command::new("git").args([...])` (argv-array form, as this codebase exclusively does everywhere already — see `evaluate_layer2`, `GitFlow`) rather than `sh -c "git ..."` string interpolation. No user/agent-controlled input reaches these commands (git refs/commit hashes are read, not written, by `build.rs`), so injection risk here is low but the argv-array discipline should be maintained for consistency |
| Preflight credential probe (`gh auth status`) leaking token material into `events.jsonl` or gate context | Information Disclosure | `gh auth status`'s stdout does not print the token itself (it prints account/scope info) — but the preflight implementation MUST NOT log raw command stdout/stderr verbatim into `events.jsonl` without the same truncation discipline used elsewhere; prefer logging only a boolean pass/fail + a short reason string, not the full probe output |
| Layer 0's `TRUST_EXTERNAL_VERIFY_ENV` approval mismatch detection (existing, D-05 preserves unchanged) | Tampering | Already correctly designed: PLAN.md-declared commands are agent-writable, so execution requires the parent process's separately-set env var to hold the EXACT approved command array — a TOCTOU-safe comparison. D-05 explicitly must not relax this when lifting the Code-stage restriction |

## Sources

### Primary (HIGH confidence — direct codebase verification)
- `crates/devflow-core/src/agent_result.rs` (full read, lines 1-1311+) — four-layer cascade, `AgentStatus` enum, `evaluate_layer0/1/2/3`
- `crates/devflow-cli/src/main.rs` (targeted reads: 580-980, 980-1330, 1230-1330, 1590-1770) — `advance()`, `launch_stage`, `run_gate`, `sequentagent`, cron-instructions call sites
- `crates/devflow-core/src/events.rs` (full read) — schema v1, `emit()`, envelope-key-wins guarantee
- `crates/devflow-core/src/agents/mod.rs` (full read) — `AgentAdapter` trait, `extra_env` default-method precedent
- `crates/devflow-core/src/monitor.rs` (full read) — shell-based exit code capture confirming exit-137 semantics
- `crates/devflow-core/src/verify.rs` (full read) — Layer 0's `TRUST_EXTERNAL_VERIFY_ENV` mechanism
- `crates/devflow-core/src/config.rs` (full read) — `devflow.toml` + env-var precedence precedent
- `crates/devflow-core/src/ship.rs` (partial read, lines 1-260) — cron-instructions machinery
- `crates/devflow-core/src/mode.rs`, `state.rs`, `stage.rs`, `gates.rs`, `agent.rs` (full reads) — `should_gate`, `consecutive_failures`, `GateAction`, `agent_running`
- `crates/devflow-core/src/version.rs` (partial read) — existing hybrid-semver version-file detection, confirms no `build.rs` infrastructure exists yet
- Direct shell verification: `rg "137"` → no results workspace-wide (confirms D-07's "currently unhandled" claim); `gh`/`git`/`cargo`/`rustc` versions probed live in the execution environment
- `.planning/phases/17-pipeline-dogfood-followup/17-CONTEXT.md`, `17-DOGFOOD-RETROSPECTIVE.md`, `.planning/STATE.md`, `.planning/config.json` (all read in full)

### Secondary (MEDIUM confidence — WebSearch cross-checked against official docs)
- [ExitStatus in std::process - Rust](https://doc.rust-lang.org/std/process/struct.ExitStatus.html) and [ExitStatusExt in std::os::unix::process - Rust](https://doc.rust-lang.org/std/os/unix/process/trait.ExitStatusExt.html) — confirmed `code()` returns `None` on signal death on Unix, `signal()` via `WTERMSIG`; used to confirm this phase does NOT need these APIs (Pitfall 1a)
- [Git - git-merge-base Documentation](https://git-scm.com/docs/git-merge-base) — confirmed `--is-ancestor` exit-code contract (0/1/other) used in D-19's Code Example and Pitfall 4
- WebSearch summary of `cargo:rustc-env` / `cargo:rerun-if-changed=.git/HEAD` build-script idiom, cross-referenced against [rust-lang/cargo's own build.rs](https://github.com/rust-lang/cargo/blob/master/build.rs) — general pattern, not this codebase's own prior art (none exists)

### Tertiary (LOW confidence)
- None — no findings in this research rely solely on unchecked WebSearch/training-data recall; all package/dependency claims are grounded in the workspace's actual `Cargo.toml` files (read directly), and all behavioral claims are grounded in direct source reads.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; every existing dependency verified directly from `Cargo.toml`
- Architecture: HIGH — the four-layer cascade, `advance()` dispatch, `AgentAdapter` trait, and event schema were all read directly from source, not inferred
- Pitfalls: HIGH — all four pitfalls are verified against the current source (grep for `"137"`, direct read of `evaluate_layer2`'s doc comment, direct read of `sequentagent`'s call sites, direct citation of git's documented exit-code contract), not speculative

**Research date:** 2026-07-18
**Valid until:** 2026-08-17 (30 days — this is a stable-domain phase with zero external dependencies; the only decay risk is the codebase itself changing under a concurrent phase, which the phase-lock/sequencing model already guards against)
