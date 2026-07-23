# Phase 21: Operator Legibility & Observability - Research

**Researched:** 2026-07-23
**Domain:** Internal Rust CLI tooling — operator-facing UX, self-diagnostic reconciliation, process bookkeeping, and a build-staleness safety gate. No new external libraries, no network I/O, no user-facing web/API surface.
**Confidence:** HIGH — every claim below was verified directly against `devflow`'s own source at HEAD (`b2e2967`), not inferred from the backlog dossiers alone. Dossier claims that could be independently re-derived from source are marked `[VERIFIED: source]`; the few that rely on the dossier's own narrative (not independently re-checked) are marked `[CITED: dossier]`.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

Ratings follow `gsd-core/references/planner-reversibility.md`.

**Scope (operator-decided this session)**

- **D-01:** Remove 999.25 (release executor) and 999.28 (`--base`) from Phase 21 — 999.25 → its own phase with an interactive discuss; 999.28 → Phase 22. — **Reversibility:** reversible (a scoping decision; nothing built yet).
- **D-02:** Phase theme is **operator legibility & observability**; renamed from "Operator Usability & Release Execution." All units must remain single-writer, reversible/detection-only, and dogfood-testable (no irreversible side effects).

**21a — Operator discoverability**
- **D-03:** Purely **additive UX** surfacing — `gate show`, rate-limit reset time in human output, in-stage progress in `status`, recovery-verb hints from a stuck state. No behavioral/correctness change to the pipeline. Sequence it first. — **Reversibility:** reversible.

**21b — Doctor planning-doc reconciliation**
- **D-04:** **Detection-only.** Add a `doctor` check that compares `ROADMAP.md`/`STATE.md` version/outcome claims against git tags (and, where cheap, published state) and **flags** drift. Do **not** auto-edit prose — same discipline 18a's reconciliation already follows. — **Reversibility:** reversible.
- **D-05:** Integrate as a new `Check` in the existing `doctor` path (`commands.rs:1121`, JSON body at `:1866`) so human and `--json` output stay consistent; do not fork a second reporter.

**21c — sequentagent second-process tracking**
- **D-06:** **Re-scope before planning.** The "monitor unrecorded" half shipped in v1.5.0 (18b). Remaining scope is *only* `sequentagent`'s second agent, which runs off the stage machine (`parallel.rs`) and has no pid record. Define what "tracked" means for a non-stage-machine handoff (a second pid file? a `sequentagent`-specific record?) as the first plan step. — **Reversibility:** reversible.

**21d — Dogfood staleness guard content-awareness**
- **D-07:** Make `embedded_commit_is_stale`'s strict-ancestor arm content-aware: when `<embedded>` is behind HEAD, run `git diff --name-only <embedded> HEAD` and filter through the existing `affects_compiled_binary`. Return `Fresh` if no build input changed, `Stale` only if one did. Reuse `affects_compiled_binary` verbatim (do not fork the predicate). Add a mixed-range test (docs + a `.rs` change → still `Stale`) so the Phase 16 false-evidence protection is preserved, and fix the block message's "is not an ancestor" wording. **Sequence 21d first (or early)** — the guard hard-blocks Phase 21's own Plan/Code/Validate stages after every `.planning/` commit, so fixing it first removes the no-op-rebuild tax for the rest of this phase. — **Reversibility:** reversible; narrows a guard without weakening its real-change detection.

**21e — ChangelogAppend content (stretch)**
- **D-08:** Stretch-only. Blocked on choosing a per-phase content source; if pulled in, that choice (SUMMARY.md extraction vs plan-diff summary) is a design decision the planner must make explicit, not assume.

### Claude's Discretion
- Exact CLI flag surface for `gate show` (positional vs `--phase`), progress representation in `status`, and whether 21a ships as one plan or splits by sub-gap — planner's call.
- Precise re-scope of 999.2, whether 21d should be its own wave-0 plan, and whether 21e is folded in at all.

### Deferred Ideas (OUT OF SCOPE)
- **999.25 — Release-cut executor → its own dedicated phase.** Irreversible (crates.io publish, signed tag, merge to main); its dossier requires its own discuss-phase on rollback semantics and a design for testing without a real publish. Do not fold back into a legibility phase.
- **999.28 — `--base` branch override → Phase 22.** Value is concurrency/stacking; belongs in the concurrency phase, not split.
- **Phase 22 concurrency:** 999.4 (version-tag contention), 999.26 (`parallel` object-store race), the concurrency half of 999.2.
- **Phase 23 test/CI:** 999.15/17/18/19/20/22.
</user_constraints>

<phase_requirements>
## Phase Requirements

This project has no `REQUIREMENTS.md` and no REQ-IDs (`.planning/REQUIREMENTS.md` does not exist). The phase's requirement surface is the operator-decided unit list in `21-CONTEXT.md`'s `<decisions>` block (D-01..D-08). Use the unit IDs below as the pseudo-requirement IDs for traceability in PLAN.md/VALIDATION.md.

| ID | Description | Research Support |
|----|-------------|------------------|
| 21a | Operator discoverability: `gate show`, rate-limit reset surfacing, in-stage progress, recovery-verb hints | Verified against `commands.rs` (`gate_list`, `status`, `cron_instruction_hints`), `pipeline_outcomes.rs` (`render_gate_context`, `handle_rate_limited_outcome`), `ship.rs` (`CronInstructions.retry_after`) — see Architecture Patterns §21a |
| 21b | Doctor: detect `ROADMAP.md`/`STATE.md` version-claim staleness against git tags | Verified against `commands.rs` `doctor`/`doctor_json_body`/`PhaseFacts` model, `ROADMAP.md` §Shipped, `STATE.md` §Completed table shapes, existing git tags — see Architecture Patterns §21b and Common Pitfalls #2 |
| 21c | sequentagent's second agent process — re-scope + add a record | Verified against `parallel.rs` (`sequentagent`, `run_agent_blocking`), `monitor.rs` (`spawn_monitor_no_advance`), `agent_result.rs` (`agent_pid_path`) — see Architecture Patterns §21c and Common Pitfalls #3/#4 |
| 21d | `embedded_commit_is_stale` ancestry arm made content-aware | Verified against `staleness.rs` full source including the exact existing test suite — see Architecture Patterns §21d and Common Pitfalls #1 (a load-bearing existing-test conflict) |
| 21e (optional) | `ChangelogAppend` real content | Verified `ship.rs:431` placeholder text and `prepend_changelog` — see Architecture Patterns §21e |
</phase_requirements>

## Summary

Phase 21 is a pure Rust-workspace, no-new-dependency phase: every unit is a targeted, single-file-or-two change inside `crates/devflow-cli/src/{commands,parallel,staleness}.rs` or `crates/devflow-core/src/ship.rs`, using patterns and helper functions that already exist in the codebase. There is no framework to choose, no crate to add, and no architecture to invent — the research task here was almost entirely "read the exact code these units extend and confirm the CONTEXT.md's claims are still accurate," which they are, with one important correction and one important scoping risk documented below.

The single most load-bearing finding: **21d's fix will change the outcome of an existing regression test** (`wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` in `staleness.rs`), because that test's fixture advances HEAD past the embedded commit using a non-Rust "unrelated follow-up" file (`b.txt`) that does not match `affects_compiled_binary`. After the content-aware fix, that specific fixture would (correctly, under the new semantics) classify as `Fresh`, not `Stale` — so the existing test must be updated (change the follow-up file to a `.rs` file, or add it to the mixed-range set) as part of 21d, not left as a silent regression. This is not a hypothetical: I read the exact test body and traced the file names against `affects_compiled_binary`'s literal match list.

The second finding worth flagging early: **21b's naive approach (compare every ROADMAP/STATE version cell against a git tag) will generate a wall of false positives** for the pre-Phase-18 history — `ROADMAP.md`'s `## Shipped` table has rows with version ranges (`0.1.0–0.6.0`), em-dashes (`—`), and duplicate versions across phases (Phase 9 and Phase 11 both claim `1.2.0`; Phases 6/7 claim `1.0.0` but no `v1.0.0` tag exists in the repo — only `v1.0.1` onward). A naive per-row check would flag all of this ancient, already-known-and-accepted history as "problems" on every `doctor` run — precisely the alert-fatigue failure mode 999.14 exists to prevent. The planner must scope the check (parseable single-semver cells only, and very likely downgrade any pre-v1.5.0 mismatch to informational rather than `Problem`).

Third: **21c's re-scoped target ("only the second agent has no pid record") undersells the actual gap** — I traced `sequentagent`'s data flow and confirmed that `devflow status`/`doctor` derive *all* phase visibility from `workflow::list_states()` (active `state-N.json` files), and `sequentagent` deliberately never persists a `State` for its phase (`parallel.rs:4-7`, "synthetic, never-persisted state... does not participate in the stage machine"). That means **both** agent A's and agent B's runs are currently invisible to `status`/`doctor` — not just the second one. D-06's "re-scope to the second agent only" is the sequencing decision (the first agent's pid *is* momentarily visible via `.devflow/phase-NN-agent-pid` while it runs, whereas nothing distinguishes "which agent, A or B" once a pid is recorded there), but the planner should explicitly re-derive and state this at plan time rather than assume the dossier's narrower framing captures the full legibility gap.

**Primary recommendation:** Sequence 21d first (per D-07/D-08 sequencing note — it taxes every subsequent stage of this very phase), then 21a (lowest risk, pure UX), then 21b and 21c in either order (both touch different files: `commands.rs`'s `doctor` vs `parallel.rs`), with 21e only if capacity remains. Do not introduce any new crate dependency for any unit — every required primitive (git shelling, JSON, pid liveness, path helpers) already exists in `devflow-core`/`devflow-cli` and should be reused, not reimplemented.

## Architectural Responsibility Map

DevFlow is a single-binary CLI (`devflow-cli`) over a domain crate (`devflow-core`). The tiers below are this project's equivalent of a web app's browser/API/DB split.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| 21a — gate/status/recovery UX surfacing | CLI display layer (`commands.rs`, `main.rs`) | Core (`gates.rs`, `ship.rs` — data already computed there) | Pure presentation of already-computed data; no new domain logic |
| 21b — planning-doc staleness detection | CLI `doctor` path (`commands.rs`) | Core (`version.rs`-style git-tag helpers, reused) | `doctor`'s existing `Check`/reconciliation composition lives in the CLI crate; git-tag lookups are thin core-adjacent helpers |
| 21c — sequentagent second-process record | CLI orchestration (`parallel.rs`) | Core (`agent_result.rs` pid-path conventions, `monitor.rs`) | The gap is specific to `parallel.rs`'s synchronous handoff; any new record type should follow `agent_result.rs`'s existing path-naming convention |
| 21d — build-staleness ancestry arm | CLI staleness gate (`staleness.rs`, `devflow-cli`) | — | Self-contained, pure-Rust + git-shelling function; no core involvement (staleness.rs already lives in `devflow-cli`, not `devflow-core`) |
| 21e — changelog content (optional) | Core (`ship.rs`) | — | `prepend_changelog`/`ChangelogAppend` are core-crate functions |

## Standard Stack

No new dependencies for any unit. All work uses primitives already present in the workspace.

### Core (already in use — no additions)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `std::process::Command` | (stdlib) | Shelling out to `git` for tag/ancestry/diff queries | Existing project-wide idiom (`staleness.rs`, `version.rs`, `git.rs`) — never introduce a git library |
| `serde_json` | workspace-pinned | `doctor --json` / event payloads | Already the sole JSON dependency across both crates |
| `clap` (derive) | workspace-pinned | New CLI subcommand/flags (e.g. `gate show`) | Already the CLI arg-parsing framework (`main.rs`) |
| `tempfile` (dev-dep) | `"3"` | Git-fixture-backed unit tests | Already used by every `staleness.rs`/`monitor.rs`/`agent_result.rs` fixture test |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `toml` (already a `devflow-core` dependency) | workspace-pinned | Reading `Cargo.toml`'s own version fields (`version.rs`) | NOT needed for 21b — `ROADMAP.md`/`STATE.md` are Markdown tables, not TOML; do not reach for this crate to parse them |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Manual line/table scanning for `ROADMAP.md`/`STATE.md` (21b) | A markdown-table parser crate | Codebase convention (see `is_self_dogfood_workspace`'s explicit "No TOML parser is used here" comment) is to hand-scan simple structured text rather than add a parsing dependency for a 2–4 column table; adding one here would be inconsistent with the rest of the codebase and unjustified for this shape of input |
| `git diff --name-only <embedded> HEAD` (21d) | Reusing `tree_has_modified_build_inputs`'s `--porcelain` parsing | Wrong tool: porcelain describes *working-tree* state, not a *committed range*; `git diff --name-only A B` is the correct, already-precedented invocation (used in the dossier's own verified repro) |

**Installation:** None required — no `Cargo.toml` changes for any unit.

**Version verification:** N/A (no new packages). Workspace Rust edition confirmed at `edition = "2024"` (workspace-level in both crate manifests), CI runs `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` (`.github/workflows/ci.yml`) — every new test/function must pass all three.

## Package Legitimacy Audit

**Not applicable.** No unit in this phase adds an external package to either `crates/devflow-core/Cargo.toml` or `crates/devflow-cli/Cargo.toml`. Confirmed by reading both manifests: `devflow-core`'s dependency set is `libc`, `serde`, `serde_json`, `toml`, `thiserror`, `tracing` (+ dev-dep `tempfile`); `devflow-cli`'s adds `clap` and similar CLI-only crates already present. All five units are implementable with these existing dependencies plus `std::process::Command`. If the planner discovers a need for something outside this set while planning (e.g. a semver-comparison crate for 21b), that would be a deviation from research and should be re-justified explicitly, not assumed.

## Architecture Patterns

### System Architecture Diagram

```
operator (terminal)
   │
   ▼
devflow CLI (main.rs — clap parses subcommand)
   │
   ├─► status / gate list / gate show(21a) ──► reads: state-NN.json, gates/*.json,
   │                                                     cron-instructions files
   │                                            (pure display — no mutation)
   │
   ├─► doctor (commands.rs:1121) ──► existing: env checks + per-active-phase
   │        │                                   reconciliation (PhaseFacts/PhaseFinding)
   │        └─► NEW (21b): planning-doc check ──► reads: ROADMAP.md, STATE.md
   │                                                 (text scan) + `git tag`/
   │                                                 `git merge-base --is-ancestor`
   │                                                 against main
   │                                            └─► emits new finding kind into
   │                                                the SAME doctor_json_body()
   │                                                document (D-05: no 2nd reporter)
   │
   ├─► sequentagent (parallel.rs:278) ──► agent A: run_agent_blocking(..., wt_a)
   │        │                                  └─► spawn_monitor_no_advance()
   │        │                                       writes phase-NN-agent-pid,
   │        │                                       stdout/exit files (synthetic,
   │        │                                       never-persisted State)
   │        │                             integrate agent A → base
   │        │                             rebase B onto base
   │        └─► agent B: run_agent_blocking(..., wt_b) ──► SAME pid/stdout/exit
   │                                             paths (keyed only by phase, not
   │                                             agent slot) ──► NEW (21c): a
   │                                             record distinguishing "this is
   │                                             agent B, not A" for status/doctor
   │                                             to surface while B is running
   │
   └─► start / advance (pipeline_launch.rs) ──► launch_stage ──► enforce_build_staleness
                                                    (staleness.rs) ──► git merge-base
                                                    --is-ancestor <embedded> HEAD
                                                    │
                                                    └─► NEW (21d): when ancestor-but-
                                                        behind, run `git diff --name-only
                                                        <embedded> HEAD`, filter through
                                                        affects_compiled_binary ──►
                                                        Fresh (docs-only) or Stale
                                                        (touches .rs/Cargo.*)
```

### Recommended Project Structure

No new files/modules. All changes land inside existing files:

```
crates/devflow-cli/src/
├── main.rs            # 21a: new `GateCmd::Show` variant
├── commands.rs         # 21a: gate_show(), status() progress line;
│                        # 21b: new planning-doc Check + finding type,
│                        #      wired into doctor()/doctor_json_body()
├── parallel.rs         # 21c: sequentagent()/run_agent_blocking() —
│                        #      re-scoped second-process record
├── staleness.rs        # 21d: embedded_commit_is_stale() content-aware arm
└── pipeline_outcomes.rs # 21a: render_gate_context / cron hint surfacing

crates/devflow-core/src/
└── ship.rs             # 21e (optional): prepend_changelog() real content
```

### Pattern 1: Detect-and-report, never auto-correct (18a discipline — governs 21b)
**What:** A `doctor` finding names the specific stale claim and where it lives; it never rewrites `ROADMAP.md`/`STATE.md` prose.
**When to use:** 21b, unconditionally per D-04.
**Example (existing precedent to mirror):**
```rust
// Source: crates/devflow-cli/src/commands.rs:1550 (check_gate_pending_without_gate)
fn check_gate_pending_without_gate(facts: &PhaseFacts) -> Option<PhaseFinding> {
    if !facts.gate_pending || !facts.open_gate_stages.is_empty() {
        return None;
    }
    Some(PhaseFinding {
        phase: facts.phase,
        severity: Severity::Problem,
        detail: format!(
            "phase {}: gate_pending is true at stage {} but no gate file is open",
            facts.phase, facts.stage
        ),
        repair: Some(format!("devflow resume --phase {}", facts.phase)),
    })
}
```
21b's new check should follow this exact shape: pure function, no I/O inside it, `Option<PhaseFinding>` (or a sibling `DocFinding` type if the data source genuinely doesn't fit `PhaseFacts` — see Open Questions), with `repair: None` (D-04: nothing to auto-repair) or at most a pointer to "update ROADMAP.md manually."

### Pattern 2: Single-document JSON composition (D-05 — governs 21b's wiring)
**What:** `doctor --json` emits exactly one JSON object; every new check's output nests as a new key under that object, never a second top-level array.
**Example:**
```rust
// Source: crates/devflow-cli/src/commands.rs:1866
fn doctor_json_body(checks: &[Check], facts: &[PhaseFacts]) -> serde_json::Value {
    serde_json::json!({
        "environment": checks_json_value(checks),
        "reconciliation": render_reconciliation_json(facts),
    })
}
```
21b's addition should extend this to a third key (e.g. `"planning_doc_staleness"`) rather than trying to force document-level findings into the phase-scoped `reconciliation` array, since (see Open Questions) planning-doc claims about *shipped* phases will often have no corresponding active `PhaseFacts` entry at all (their `state-NN.json` is long gone — confirmed `workflow::remove_state`/cleanup exists and is called after shipping).

### Pattern 3: Content-aware staleness narrowing (D-07 — the exact 21d shape)
**What:** Filter a set of changed paths through the existing `affects_compiled_binary` predicate before treating a commit range as staleness-relevant.
**Existing precedent (the dirty-tree arm, already content-aware since 17-10):**
```rust
// Source: crates/devflow-cli/src/staleness.rs:106 (tree_has_modified_build_inputs)
fn tree_has_modified_build_inputs(execution_root: &Path) -> Option<bool> {
    let status = run_git_stdout(execution_root, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Some(false);
    }
    Some(
        status
            .lines()
            .any(|line| porcelain_tracked_path(line).is_some_and(affects_compiled_binary)),
    )
}
```
**21d's new ancestry-arm equivalent** (not yet in source — this is the shape to implement per D-07, using `git diff --name-only` instead of `--porcelain` since the input is a committed range, not a working tree):
```rust
// Sketch only — verify exact signature/call sites against staleness.rs at plan time.
fn ancestry_range_affects_build(execution_root: &Path, embedded_commit: &str) -> bool {
    run_git_stdout(execution_root, &["diff", "--name-only", embedded_commit, "HEAD"])
        .map(|out| out.lines().any(affects_compiled_binary))
        .unwrap_or(true) // fail toward Stale (never a false Fresh) if git itself errors
}
```
`embedded_commit_is_stale`'s exit-0-but-not-HEAD branch (currently unconditional `Staleness::Stale`) should call this and only return `Stale` if it returns `true`, else `Fresh`. **Do not** touch the exit-1 (Ahead/Stale-divergent) branches or the `Indeterminate` fallbacks — those are out of scope per D-07 and per the codebase's own Pitfall-4 discipline ("Indeterminate must never hard-block").

### Anti-Patterns to Avoid
- **Forking a second `doctor` reporter for 21b:** D-05 explicitly forbids this; the human/`--json` outputs must stay one coherent document.
- **Auto-editing `ROADMAP.md`/`STATE.md` prose in 21b:** explicitly out of scope (D-04); this is a much bigger, riskier feature the dossier itself flags as deliberately deferred.
- **Routing sequentagent's second-agent record through `State`/`save_state` (21c):** already investigated and rejected during Phase 19 (D-14: "the retrospective proposal to add [a `save_state` chokepoint] was verified wrong" for `run_agent_blocking`'s synthetic, never-persisted state) — a comment at `parallel.rs:4-7` reiterates this. Any new 21c record must be a lightweight sibling file/marker, not a `State` persistence path.
- **Widening 21d's fix beyond the strict-ancestor arm:** the `Ahead` and `Indeterminate` branches of `embedded_commit_is_stale`/`combined_staleness` must remain byte-identical; only the `Ok(Some(0))` "exit 0 but not exact HEAD match" arm changes.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Detecting a modified build-relevant file in a diff | A new file-extension/pattern matcher | `affects_compiled_binary` (`staleness.rs:146`), reused verbatim | D-07 explicitly mandates reuse; it already correctly excludes `.planning/`, `CHANGELOG.md`, and matches `.rs`/`Cargo.toml`/`Cargo.lock`/`build.rs`/`rust-toolchain.toml` |
| Pid liveness probing | A new `is_process_alive` helper | `devflow_core::agent::agent_running(pid)` | Already used by `status`, `doctor`'s `check_dead_agent`/`check_dead_monitor`, and would need to be for any new 21c liveness display |
| Git ancestry/diff shelling | A git library (e.g. `git2`) | `std::process::Command::new("git").args([...])`, matching `run_git_stdout`'s idiom | Zero new dependency surface; every existing staleness/version function already does this |
| Markdown table parsing (21b) | A markdown/table-parsing crate | Manual line scanning keyed on the table's fixed column shape (mirrors `is_self_dogfood_workspace`'s deliberate no-parser convention) | The tables are small, fixed-shape, and already hand-written by the project itself; a parsing dependency is unjustified overhead |
| Full untruncated gate context (21a) | A new gate-context storage field | `Gates::list_open`/`GateFile.context` (`gates.rs:24`), already stores the FULL string — only `gate_list`'s *display* truncates it to 100 chars via `render_gate_context` | No new data model needed; `gate show` is purely a new, untruncated read/print path over data that already exists on disk |

**Key insight:** Every one of this phase's five units is additive glue over data or predicates the codebase already computes correctly. The risk is not "what do we build" but "don't accidentally rebuild something that already exists two files over" — confirmed for all five units above.

## Common Pitfalls

### Pitfall 1: 21d's fix silently flips an existing regression test's expected outcome
**What goes wrong:** `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` (`staleness.rs:742`) builds a fixture where the embedded commit is a strict ancestor of HEAD, and the only intervening commit adds `b.txt` — a file that is **not** `.rs` and **not** in `affects_compiled_binary`'s `BUILD_AFFECTING_FILES` list. The test currently asserts `Staleness::Stale`. After 21d's content-aware fix, this exact fixture would (correctly, under the new semantics) become `Fresh`, silently breaking this test unless it is updated.
**Why it happens:** The WR-01 regression test was written to prove "any forward HEAD movement past the embedded commit is Stale," using an arbitrary non-source file as a stand-in for "any commit." 21d deliberately narrows that guarantee to "forward movement that touches build inputs is Stale" — the two are incompatible for this specific fixture.
**How to avoid:** Update the WR-01 test's second commit to touch a `.rs` file (or a `Cargo.toml`/`build.rs`) instead of `b.txt`, preserving its original intent (a genuine code change after the build must still hard-block), and add a **separate** new test for the docs-only-range → `Fresh` case (the actual 21d/999.29 scenario) plus the CONTEXT-mandated mixed-range test (docs + `.rs` → still `Stale`).
**Warning signs:** `cargo test --workspace` failing on `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` immediately after implementing the content-aware arm — this is expected and must be fixed by editing the fixture, not by reverting the new logic.

### Pitfall 2: Naive version-claim reconciliation (21b) will flag decades of accepted history
**What goes wrong:** `ROADMAP.md`'s `## Shipped` table and `STATE.md`'s `## Completed` table both contain rows that cannot cleanly resolve to "does tag X exist and is it reachable": version ranges (`0.1.0–0.6.0` for Phases 1–5), em-dash placeholders (`—` for Phases 0–5, 10, 12–17), and duplicate version claims across phases (Phase 9 and Phase 11 both list `1.2.0`; Phases 6/7 both list `1.0.0`, but `git tag --list` on this repo shows tags starting at `v1.0.1` — **no `v1.0.0` tag exists at all**). A literal "every row must have a matching, reachable tag" check will report several `Problem`-severity findings on every single `doctor` run for phases that shipped over a month ago and are not actionable.
**Why it happens:** Pre-Phase-18, the project did not consistently tag every phase; per-phase version tracking only became disciplined starting around Phase 18/19's release-integrity work.
**How to avoid:** Scope the check to rows whose version cell matches a strict `^v?\d+\.\d+\.\d+$` pattern (skip ranges/em-dashes outright), and strongly consider either (a) only checking rows from a configurable "since phase N" cutoff, or (b) reporting historical (pre-cutoff) mismatches at `Severity::Warn`/informational rather than `Problem`, so the signal-to-noise ratio doesn't erode trust in `doctor` — exactly the failure mode 999.14 itself was filed to prevent (see 999.14's own dossier: "17-REVIEW.md WR-06 already named once" — this is a *recurring* class of doc-trust erosion, and a noisy detector re-introduces the same class it's meant to catch).
**Warning signs:** `doctor`'s reconciliation output growing to a dozen+ lines the moment 21b ships, with most of them about phases shipped 2026-06 or earlier.

### Pitfall 3: sequentagent's synthetic State must not be persisted to satisfy 21c
**What goes wrong:** The obvious-looking fix — "just call `workflow::save_state` for the synthetic per-agent `State` in `run_agent_blocking`" — was already proposed and rejected during Phase 19 planning (D-14): it was verified wrong because `run_agent_blocking`'s state is deliberately synthetic and never-persisted, and forcing persistence would create a `state-NN.json` for a phase that is simultaneously being driven by two agents in two different worktrees with two different branches — an ill-defined `State` (which worktree? which stage?) that would corrupt `status`/`doctor`'s single-state-per-phase assumption.
**Why it happens:** 18b's `State.monitor_pid` precedent makes "just persist it like the monitor pid" look like a drop-in fix, but that precedent applies to the ordinary single-agent pipeline's `State`, which sequentagent explicitly does not use.
**How to avoid:** Design a `sequentagent`-specific, lightweight record (e.g., a new file under `.devflow/` keyed by phase + agent slot, following `agent_result.rs`'s existing path-naming convention like `agent_pid_path`) that `status`/`doctor` can optionally read, without routing through `State`/`save_state`. Define this data model explicitly as the plan's first task, per D-06.

### Pitfall 4: `agent_pid_path`/stdout/exit files are keyed only by phase, not by agent slot
**What goes wrong:** Both `run_agent_blocking` calls in `sequentagent` (once for agent A, once for agent B) write to the exact same `phase-NN-agent-pid`/`phase-NN-stdout`/`phase-NN-exit` paths (`monitor.rs:91-94`, itself calling `agent_result::{stdout_path, stderr_path, exit_code_path, agent_pid_path}`, all keyed only by `(project_root, phase)`). Since the two agents run strictly sequentially (never concurrently), this doesn't cause a live collision, but it does mean: (a) nothing on disk distinguishes "this pid belongs to agent A" from "this pid belongs to agent B", and (b) `archive_phase_files` (called at the top of each `run_agent_blocking` invocation) archives the prior agent's captures into `history/phase-NN/` before the next agent's monitor overwrites the live files — so agent A's capture is recoverable from history, but nothing labels which history generation was "agent A" vs the ordinary pipeline's own earlier runs of the same phase number.
**Why it happens:** `agent_result.rs`'s path helpers predate `sequentagent`'s two-agent-per-phase design and were never extended with an agent-slot dimension.
**How to avoid:** If 21c's chosen design adds a second pid record, name it distinctly (e.g., include the agent letter or `AgentKind` in the filename) so `status`/`doctor` can report "sequentagent: agent B running (pid P)" rather than a bare, unlabeled pid.

### Pitfall 5: Rate-limit reset time is already computed — the 21a fix is presentation-only
**What goes wrong:** It's tempting to re-derive rate-limit reset detection for `status`/gate output, duplicating `agent_result::detect_rate_limit`/`detect_claude_rate_limit`/`detect_codex_rate_limit`.
**Why it happens:** The dossier's phrasing ("Rate-limit reset times exist only inside raw agent JSON") reads as if no extraction exists yet.
**How to avoid:** `CronInstructions.retry_after: String` (`ship.rs:20`) is already populated from exactly this detection path (via `retry_after_from_reason` in `parallel.rs:404` and the direct call site in `pipeline_outcomes.rs`'s `handle_rate_limited_outcome`) and written to a per-phase cron-instructions file. The actual gap is that `cron_instruction_hints` (`commands.rs:893`) prints only `"Cron instruction pending (phase N): hermes cron create --from-devflow ..."` — it never reads/prints the `retry_after` field it already has access to via `CronInstructions`. The fix is: read `instructions.retry_after` and include it in the printed hint. No new detection logic needed.
**Warning signs:** A plan task that proposes writing a new stdout/JSON scanner for rate-limit timestamps — that would duplicate existing, already-tested logic in `agent_result.rs`.

## Code Examples

### `gate show` — untruncated context (21a), mirroring `gate_respond`'s stage-resolution idiom
```rust
// Source: crates/devflow-cli/src/commands.rs:685-694 (gate_respond's existing
// auto-resolve-single-open-gate pattern — reuse this exact shape for `gate show`)
let stage = match stage {
    Some(stage) => stage,
    None => {
        let open: Vec<_> = Gates::list_open(project_root)
            .into_iter()
            .filter(|g| g.phase == phase)
            .collect();
        match open.as_slice() {
            [] => { /* ...no open gate error... */ }
            // exactly one → auto-resolve; more than one → require --stage
            _ => { /* ... */ }
        }
    }
};
```
A new `gate_show(project_root, phase, stage: Option<Stage>)` should reuse this resolution shape, then print `gate.context` (the full `String`, from `Gates::list_open`/`GateFile`) without passing it through `render_gate_context`'s 100-char cap.

### `doctor --json` extension shape (21b)
```rust
// Source: crates/devflow-cli/src/commands.rs:1866-1871 — extend this exact function
fn doctor_json_body(checks: &[Check], facts: &[PhaseFacts]) -> serde_json::Value {
    serde_json::json!({
        "environment": checks_json_value(checks),
        "reconciliation": render_reconciliation_json(facts),
        // NEW: "planning_doc_staleness": render_planning_doc_findings_json(&doc_findings),
    })
}
```

### git-tag existence + reachability check (21b — new helper, following `version.rs`'s idiom)
```rust
// Sketch, following the exact Command::new("git").args([...]) idiom already
// used in version.rs's count_git_tags and staleness.rs's run_git_stdout.
fn tag_exists_and_reachable(project_root: &Path, tag: &str, base_branch: &str) -> bool {
    let exists = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &format!("refs/tags/{tag}")])
        .current_dir(project_root)
        .output()
        .is_ok_and(|o| o.status.success());
    exists
        && std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", tag, base_branch])
            .current_dir(project_root)
            .output()
            .is_ok_and(|o| o.status.success())
}
```
Note: `ROADMAP.md`/`STATE.md` version cells are bare (`1.7.0`), while git tags carry a `v` prefix (`v1.7.0`) — confirmed by inspecting both the tables and `git tag --list`. Any 21b implementation must normalize this (`format!("v{cell}")`) before the lookup.

## State of the Art

Not applicable in the traditional sense (no external ecosystem/framework churn to track for this phase) — the relevant "state of the art" is entirely this project's own prior fixes, which the CONTEXT.md already correctly cites:

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Dirty-tree staleness treated any modified file as Stale | Filtered through `affects_compiled_binary` | 17-10 | The precedent 21d must mirror for the ancestry arm |
| `doctor --json` printed two concatenated top-level JSON arrays | Single `{"environment", "reconciliation"}` object | 18-fix WR-01 | The precedent 21b must extend, not fork |
| `main.rs`-monolith command dispatch | Split into `commands.rs`/`parallel.rs`/`pipeline_*.rs`/`staleness.rs` | Phase 19 (999.8) | All five units' target files are the *post-split* locations named in CONTEXT.md's canonical refs — confirmed current |

**Deprecated/outdated:** None relevant — no unit in this phase touches deprecated code paths.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | 21c's chosen "tracking" mechanism should be a new, distinctly-named pid/record file (not routed through `State`) | Architecture Patterns / Pitfall 3 | Low — this is a design recommendation, not a verified fact; the planner may find a cleaner shape (e.g. an in-memory-only display during the synchronous run is sufficient if the operator-legibility gap is scoped to "post-crash forensics" rather than "live status"). Flagged `[ASSUMED]` because the exact data model is explicitly left to the plan (D-06) |
| A2 | 21b's version-claim scoping should downgrade pre-v1.5.0 mismatches to informational/Warn rather than omit them entirely | Common Pitfalls #2 | Medium — an alternative valid design is to hard-cutoff (skip entirely) rows before a configured phase number; both are reasonable, this research recommends but does not mandate one over the other |
| A3 | 21e (if pulled in) should extract content from `SUMMARY.md` rather than a plan-diff, if forced to choose | Phase Requirements / D-08 note | Low — 21e is optional/stretch; CONTEXT.md D-08 already flags this as an open design decision the planner must make explicit, not this research's call. Not asserted as a recommendation, only noted as the two known options from the 999.5 dossier |

**If this table is empty:** N/A — three low/medium-risk items are logged above; none touch security, compliance, or irreversible behavior, consistent with D-02's constraint that every unit stay reversible/detection-only.

## Open Questions (RESOLVED)

> Resolved at plan time (2026-07-23), verified by the plan-checker:
> - **Q1** → 21-03 uses a new sibling `PlanningDocFinding` type composed into `doctor_json_body` as a third top-level key (shipped phases have no active `PhaseFacts`).
> - **Q2** → 21-04 takes the **narrow** reading (label the running slot A/B, surfaced in `status` only), with the tradeoff flagged explicitly per D-06 rather than silently assumed.
> - **Q3** → 21-02 uses positional `phase` + optional `--stage`, matching `gate approve`/`gate reject`.

1. **Does 21b's finding type reuse `PhaseFinding`/`PhaseFacts`, or need a new sibling type?**
   - What we know: `collect_phase_facts`/`PhaseFacts` are built exclusively from `workflow::list_states()` — i.e., only phases with an active `state-NN.json`. Confirmed a `remove_state`/cleanup path exists (`workflow.rs`), meaning shipped/completed phases' state files are eventually removed.
   - What's unclear: Whether the planner wants planning-doc findings scoped only to phases that still have an active state (a much smaller, less useful set — most `ROADMAP.md`/`STATE.md` version claims are about *already-shipped* phases with no active state) or a new document-level fact-gathering pass entirely independent of `PhaseFacts`.
   - Recommendation: A new, small `PlanningDocFinding`-like type (or a reused `PhaseFinding` with `phase` populated from the parsed table row, independent of whether an active `PhaseFacts` exists for it) composed into `doctor_json_body` as a third top-level key (Pattern 2 above). This avoids forcing document-level facts through the active-phase-only `collect_phase_facts` pipeline.

2. **Does 21c's re-scoped "second agent only" framing fully address the underlying legibility gap, or should the plan re-verify and potentially widen it?**
   - What we know: `devflow status`/`doctor` show nothing for a phase actively driven by `sequentagent` (neither agent A nor agent B), because no `State` is ever persisted for it. D-06 explicitly re-scopes this to "sequentagent's second agent" only, since 18b already covers "the monitor unrecorded" for the ordinary pipeline.
   - What's unclear: Whether D-06's authors intended "second agent" narrowly (literally: give agent B a record, since agent A's pid is at least visible via `.devflow/phase-NN-agent-pid` while it runs) or whether re-scoping at plan time (as D-06 instructs) should surface that agent A is equally invisible to `status`/`doctor` today.
   - Recommendation: The plan's first task (per D-06) should explicitly restate this finding — confirmed in this research — and decide whether the fix covers "distinguish A from B while either runs" (narrow, matches D-06's literal text) or "make sequentagent visible to `status`/`doctor` at all" (broader, arguably the actual operator-legibility goal). Do not silently assume the narrow reading without flagging the tradeoff to the operator.

3. **Should `gate show`'s flag surface be positional (`devflow gate show <phase>`) or `--phase`?**
   - What we know: `gate list` takes no phase argument (lists all open gates); `gate approve`/`gate reject` take a positional `phase: u32` (`main.rs:283-296`).
   - What's unclear: Consistency argument favors positional (matching `approve`/`reject`); CONTEXT.md explicitly leaves this to the planner (Claude's Discretion).
   - Recommendation: Match `gate approve`/`gate reject`'s existing positional-phase convention for consistency, with an optional `--stage` exactly like `gate approve`'s `stage_option`.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | All five units (ancestry/diff/tag shelling) | ✓ | present in dev environment (already required by every existing `staleness.rs`/`version.rs` function) | — |
| `cargo` (test/clippy/fmt) | Validation of every unit | ✓ | workspace-pinned edition 2024 | — |
| Network / crates.io | None of the five units require it | N/A | — | — |

**Missing dependencies with no fallback:** None.
**Missing dependencies with fallback:** None — this phase has no network or optional-tool dependencies; `git` and `cargo` are both already hard requirements of the existing codebase this phase extends.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in Rust test harness; no external test framework) |
| Config file | none — tests are colocated `#[cfg(test)] mod tests` blocks per source file (see `staleness.rs:355` onward, `commands.rs:1873` onward, `parallel.rs:476` onward) |
| Quick run command | `cargo test --workspace <module_path>::` (e.g. `cargo test --workspace staleness::tests::` for 21d, `cargo test --workspace commands::tests::` for 21a/21b, `cargo test --workspace parallel::tests::` for 21c) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| 21a | `gate show <phase>` prints full, untruncated `GateFile.context` | unit (CLI arg parsing) + integration (fixture gate file → stdout) | `cargo test --workspace commands::tests::gate_show` (name TBD by plan) | ❌ Wave 0 — new test |
| 21a | `status` surfaces `CronInstructions.retry_after` in its rate-limit hint | unit | `cargo test --workspace commands::tests::cron_instruction_hints` (extend existing coverage) | ✅ — `cron_instruction_hints` already has a call site; extend, don't create from scratch |
| 21a | Recovery-verb hint reachable from every stuck-state path, not only the one `status` already prints | unit | `cargo test --workspace commands::tests::` (extend `Liveness::Stuck` coverage) | ✅ partial — `status`'s Stuck→`devflow resume` hint already exists and is tested; verify `doctor`'s equivalent path also hints correctly |
| 21b | `doctor`/`doctor --json` flags a `ROADMAP.md`/`STATE.md` version claim whose tag doesn't exist or isn't reachable from `main` | unit (pure fn over parsed rows + git fixture) + fixture (git-repo-in-tempdir with a fabricated tag mismatch) | `cargo test --workspace commands::tests::` (new `planning_doc_staleness_*` tests, mirroring `reconcile_phase_flags_*` naming) | ❌ Wave 0 — new test file/module section |
| 21b | Historical/legacy rows (ranges, em-dashes, duplicate versions) do NOT produce `Problem`-severity noise | unit | `cargo test --workspace commands::tests::` (new test asserting the em-dash/range rows are skipped or downgraded) | ❌ Wave 0 — new test, directly derived from Pitfall #2 above |
| 21c | sequentagent's second agent (agent B) has a discoverable record distinct from agent A while running | unit (pure fn) + integration (spawn two `sh` fixtures via existing `monitor.rs` test idiom, e.g. `sequentagent_...` style) | `cargo test --workspace parallel::tests::` (new) | ❌ Wave 0 — new test |
| 21d | Docs-only commit range (embedded ancestor of HEAD) → `Fresh`, not `Stale` | unit (git fixture, mirrors `init_repo_with_diverged_commit`) | `cargo test --workspace staleness::tests::` (new `docs_only_range_is_fresh` style name) | ❌ Wave 0 — new test |
| 21d | Mixed range (docs + a `.rs` change) → still `Stale` | unit (git fixture) | `cargo test --workspace staleness::tests::` (new, explicitly mandated by D-07) | ❌ Wave 0 — new test |
| 21d | Existing `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` still passes (fixture updated to use a `.rs` follow-up file) | regression | `cargo test --workspace staleness::tests::wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` | ✅ exists — MUST be edited, not left as-is (see Pitfall #1) |
| 21e (optional) | Changelog entry contains real per-phase content, not the placeholder string | unit | `cargo test --workspace ship::tests::` (extend `prepend_changelog` coverage) | ✅ partial — `prepend_changelog` already has test coverage; extend for the new content source once D-08's choice is made |

### Sampling Rate
- **Per task commit:** targeted module test, e.g. `cargo test --workspace staleness::` or `cargo test --workspace commands::`
- **Per wave merge:** `cargo test --workspace` (full suite) + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --check`
- **Phase gate:** Full suite green (CI parity: `cargo test --workspace`, clippy, fmt) before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `staleness.rs` — new fixture-backed tests for docs-only-range→Fresh and mixed-range→Stale (21d); update the WR-01 test fixture (Pitfall #1)
- [ ] `commands.rs` — new tests for the planning-doc staleness check (21b), including the legacy-row-noise regression test (Pitfall #2)
- [ ] `parallel.rs` — new test(s) for whatever second-process record 21c's plan defines (data model TBD — Open Question #2)
- [ ] `commands.rs` — new/extended test for `gate show`'s untruncated output and the rate-limit-reset hint surfacing (21a)
- [ ] No new test framework or fixture harness needed — every gap above is additive coverage inside an existing, already-conventioned test module

*(Existing test infrastructure — `tempfile`-backed git fixtures, `#[cfg(test)]` colocated modules — covers the shape of every new test needed; only the specific new test bodies are gaps.)*

## Security Domain

`security_enforcement` is absent from `.planning/config.json` (treated as enabled per instructions), so this section is included even though DevFlow is a local developer CLI with no network-facing auth/session surface.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | Local CLI tool; no user accounts or credentials handled by any of these five units |
| V3 Session Management | No | No session concept in this phase's scope |
| V4 Access Control | No | Single-operator local tool; no multi-tenant access boundaries |
| V5 Input Validation | Yes (narrow) | CLI argument parsing via `clap` derive (existing pattern); any new git-shelled command must use `Command::new("git").args([...])` (argv array), never `Command::new("sh").arg("-c").arg(interpolated_string)` — the interpolation-vulnerable pattern already exists in this codebase (`quality_check`-style commands at `commands.rs:1080`) but must NOT be the pattern any of these five units follow, since none of them need a shell |
| V6 Cryptography | No | No cryptographic material touched by any of these five units (signing lives in `release --check`'s `check_signing`, out of this phase's scope) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Command injection via shelled git/tag names built from untrusted input | Tampering | Every new git invocation (21b's tag lookups, 21d's diff range) MUST use `Command::new("git").args([...])` with each argument as a separate `&str`, mirroring `run_git_stdout`'s existing idiom — never build a shell string and pass it to `sh -c` |
| Filesystem-path / OS-username leakage into `.devflow/events.jsonl` | Information Disclosure | Established discipline from WR-02 (18-fix): event payloads must carry only phase numbers, stage names, and pids — never `Path::display()` output. Any new event emitted by 21b/21c must follow this same discipline (the existing `self_dogfood_stale_blocked` event at `staleness.rs`'s `enforce_build_staleness` is the canonical example to mirror) |
| A doctor/reconciliation finding accidentally becoming a decision-making input that silently auto-corrects planning docs | Tampering (of trust, not data) | D-04 explicitly forbids auto-correction; enforce this at code-review time — 21b's new check function must have no write path to `ROADMAP.md`/`STATE.md` whatsoever, only reads |
| A malformed/adversarial `ROADMAP.md`/`STATE.md` table row (e.g. a crafted version string) causing a panic in 21b's parser | Denial of Service (local) | Parse defensively — treat any row that doesn't match the expected shape as "skip, not a finding," mirroring `Gates::list_open`'s existing "any unparsable file is skipped — listing must degrade, not die" discipline |

## Sources

### Primary (HIGH confidence — direct source inspection this session)
- `crates/devflow-cli/src/staleness.rs` (full file, 1284 lines) — `Staleness`, `embedded_commit_is_stale`, `combined_staleness`, `tree_has_modified_build_inputs`, `affects_compiled_binary`, `enforce_build_staleness`, and the full existing test suite including `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks` and `init_repo_with_diverged_commit`
- `crates/devflow-cli/src/commands.rs` — `doctor` (:1121), `doctor_json_body` (:1866), `Check`, `PhaseFacts`/`PhaseFinding`/`Severity`, `reconcile_phase` and its five existing checks, `collect_phase_facts`/`build_phase_facts`, `status` (:528), `gate_list`/`gate_respond` (:654-694), `agent_pid_from_file` (:888), `cron_instruction_hints` (:893)
- `crates/devflow-cli/src/parallel.rs` (full file, 530 lines) — `sequentagent`, `run_agent_blocking`, `parse_phase_agent_pairs`, `split_two_agents`, `retry_after_from_reason`
- `crates/devflow-core/src/monitor.rs` (lines 1-120) — `spawn_monitor`/`spawn_monitor_no_advance`, path derivation from `agent_result::{stdout_path, stderr_path, exit_code_path, agent_pid_path}`
- `crates/devflow-core/src/agent_result.rs` (lines 870-960) — path helper definitions, `archive_phase_files`
- `crates/devflow-core/src/gates.rs` (lines 1-160) — `GateFile`, `OpenGate`, `Gates::list_open`, confirming full untruncated `context` is already stored
- `crates/devflow-core/src/ship.rs` — `CronInstructions`/`retry_after` field, `prepend_changelog` (the `ship.rs:431` placeholder)
- `crates/devflow-core/src/version.rs` — `count_git_tags`, existing git-shelling idioms to mirror for 21b's tag lookups
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `render_gate_context` (the 100-char truncation), `handle_rate_limited_outcome`
- `crates/devflow-cli/src/main.rs` — full `Command`/`GateCmd` enum (confirming no existing `gate show` variant)
- `.planning/ROADMAP.md` and `.planning/STATE.md` — exact table shapes for 21b's target data
- `git tag --list` (executed this session) — confirmed actual tag inventory (`v1.0.1` through `v1.7.0`; no `v1.0.0`)
- `crates/devflow-core/Cargo.toml`, `crates/devflow-cli/Cargo.toml` — confirmed no new dependency is needed for any unit
- `.github/workflows/ci.yml` — confirmed CI's exact `cargo test --workspace` / clippy / fmt gates

### Secondary (MEDIUM confidence)
- `.planning/phases/999.3-cli-operator-discoverability/CONTEXT.md`, `999.14-...`, `999.2-...`, `999.29-...`, `999.5-...` — backlog dossiers, cross-checked against source above (all claims independently re-verified except where noted `[CITED: dossier]` above)

### Tertiary (LOW confidence)
- None used — every substantive claim in this document was checked directly against source or git output this session.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; confirmed by reading both `Cargo.toml` manifests directly
- Architecture: HIGH — every pattern cited is quoted verbatim from source read this session, not reconstructed from memory or the dossiers alone
- Pitfalls: HIGH — Pitfall #1 (the WR-01 test conflict) and Pitfall #2 (legacy version-table noise) were discovered independently this session by tracing the actual fixture code and the actual `ROADMAP.md`/`STATE.md` table contents, not copied from any dossier

**Research date:** 2026-07-23
**Valid until:** Effectively indefinite for the architectural findings (internal, stable code); re-verify git tag inventory and `ROADMAP.md`/`STATE.md` table contents at plan time if more than a few days pass, since both are live, actively-changing project state.
