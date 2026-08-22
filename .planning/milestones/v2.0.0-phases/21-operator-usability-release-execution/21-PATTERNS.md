# Phase 21: Operator Legibility & Observability - Pattern Map

**Mapped:** 2026-07-23
**Files analyzed:** 5 (all modified, none new — no new modules for this phase)
**Analogs found:** 5 / 5 (all analogs are in the SAME file as the new code, verified live against HEAD)

All anchors below were re-verified directly against source this session (not taken on faith from CONTEXT.md/RESEARCH.md) — line numbers match current HEAD exactly.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog (same file, different function) | Match Quality |
|--------------------|------|-----------|--------------------------------------------------|---------------|
| `crates/devflow-cli/src/commands.rs` (21a: `gate_show`, `status` progress, `cron_instruction_hints`) | CLI display/controller | request-response (read-only presentation) | `gate_respond` (:679), `gate_list` (:654), `status` (:528) — all in same file | exact |
| `crates/devflow-cli/src/main.rs` (21a: `GateCmd::Show` variant) | route/CLI arg definition | request-response | `GateCmd::Approve` (:283) | exact |
| `crates/devflow-cli/src/commands.rs` (21b: new doctor `Check`/finding + `doctor_json_body` extension) | service/controller | CRUD-like detection (read-compare-report) | `check_gate_pending_without_gate` (:1553), `doctor_json_body` (:1866) — same file | exact |
| `crates/devflow-cli/src/parallel.rs` (21c: sequentagent second-process record) | service/event-driven | event-driven (process lifecycle bookkeeping) | `run_agent_blocking` (:155), `agent_result::agent_pid_path` (agent_result.rs:893) | role-match |
| `crates/devflow-cli/src/staleness.rs` (21d: content-aware ancestry arm) | utility/service | transform (git diff → boolean predicate) | `tree_has_modified_build_inputs` (:106) + `affects_compiled_binary` (:146) — same file | exact |
| `crates/devflow-core/src/ship.rs` (21e optional: `prepend_changelog` real content) | service | transform | `prepend_changelog` itself (:428) — modify in place | exact |

## Pattern Assignments

### 21a — `crates/devflow-cli/src/commands.rs` + `main.rs` (gate show, rate-limit hint, progress, recovery hints)

**Analog 1 — `GateCmd` enum, positional-phase CLI convention** (`main.rs:275-296`):
```rust
enum GateCmd {
    /// List gates awaiting a response.
    List {
        #[arg(default_value = ".")]
        project: PathBuf,
    },
    /// Approve an open gate — the workflow advances.
    Approve {
        /// Phase whose gate to approve.
        phase: u32,
        #[arg(value_name = "STAGE_OR_PROJECT")]
        stage: Option<String>,
        #[arg(value_name = "PROJECT")]
        legacy_project: Option<PathBuf>,
        #[arg(long = "stage")]
        // ...
    },
    // Reject { ... } follows the same shape
}
```
A new `Show { phase: u32, #[arg(long = "stage")] stage: Option<String> }` variant should match `Approve`'s positional-phase + `--stage` shape (per RESEARCH Open Question #3's recommendation — consistency over `list`'s no-arg shape).

**Analog 2 — auto-resolve-single-open-gate idiom** (`commands.rs:679-711`, `gate_respond`):
```rust
pub(crate) fn gate_respond(
    project_root: &Path,
    phase: u32,
    stage: Option<Stage>,
    approved: bool,
    note: Option<String>,
) -> Result<(), CliError> {
    let stage = match stage {
        Some(stage) => stage,
        None => {
            let open: Vec<_> = Gates::list_open(project_root)
                .into_iter()
                .filter(|g| g.phase == phase)
                .collect();
            match open.as_slice() {
                [] => {
                    return Err(CliError::Message(format!(
                        "no open gate for phase {phase} — see `devflow gate list`"
                    )));
                }
                [one] => one.stage,
                many => {
                    return Err(CliError::Message(format!(
                        "phase {phase} has several open gates ({}) — pass --stage",
                        many.iter().map(|g| g.stage.to_string()).collect::<Vec<_>>().join(", ")
                    )));
                }
            }
        }
    };
    // ...
}
```
`gate_show(project_root, phase, stage: Option<Stage>)` must reuse this EXACT stage-resolution block verbatim, then instead of writing a `GateResponse`, look up the matching `OpenGate` from `Gates::list_open` and print `gate.context` (the full untruncated `String` — already stored on disk, confirmed in `gates.rs`) directly, bypassing `render_gate_context`'s 100-char cap entirely.

**Analog 3 — `gate_list`'s truncated-context display, to contrast with (`commands.rs:654-675`):**
```rust
pub(crate) fn gate_list(project_root: &Path) -> Result<(), CliError> {
    let open = Gates::list_open(project_root);
    if open.is_empty() {
        println!("no open gates");
        return Ok(());
    }
    println!("{:<6} {:<9} {:<9} CONTEXT", "PHASE", "STAGE", "AGE");
    for gate in &open {
        let context = render_gate_context(&gate.context, 100);
        println!("{:<6} {:<9} {:<9} {context}", gate.phase, gate.stage.to_string(), recover::format_age(&gate.timestamp));
    }
    println!("\nanswer with: devflow gate approve <phase> [--note ...] | devflow gate reject <phase> --note ... (note with \"abort\" ends the phase)");
    Ok(())
}
```
`gate show`'s hint footer should follow this same pattern (a `println!` telling the operator the next command), but pointing at itself is unnecessary — instead it's the natural place to ALSO print recovery-verb hints per D-03 if the gate is on a stuck phase.

**Analog 4 — rate-limit reset surfacing gap** (`commands.rs:893-904`, `cron_instruction_hints` — THE function to edit, not rewrite):
```rust
fn cron_instruction_hints(project_root: &Path) -> Vec<String> {
    devflow_core::ship::list_cron_instructions(project_root)
        .iter()
        .map(|instructions| {
            format!(
                "Cron instruction pending (phase {}): hermes cron create --from-devflow {}",
                instructions.phase,
                project_root.display()
            )
        })
        .collect()
}
```
`CronInstructions` (`crates/devflow-core/src/ship.rs:12-20`) already has a public `pub retry_after: String` field populated by the existing detection path. **Do not build a new scanner** — just add `instructions.retry_after` into the format string, e.g. `"...--from-devflow {} (rate-limit resets: {})"`. Confirmed no dead-code path needs touching; `list_cron_instructions` already returns fully-populated structs.

**Analog 5 — in-stage progress / recovery hints in `status`** (`commands.rs:528-597`):
```rust
pub(crate) fn status(project_root: &Path) -> Result<(), CliError> {
    let states = workflow::list_states(project_root);
    // ... per-state loop ...
    println!("  stage: {} | mode: {} | gate: {}", state.stage, state.mode, gate);
    // ...
    let phase_liveness = liveness(state.monitor_pid, monitor_alive, agent_alive);
    println!("  liveness: {}", phase_liveness.describe());
    if phase_liveness == Liveness::Stuck {
        println!("    → devflow resume --phase {}", state.phase);
    }
}
```
The existing `Stuck` → `devflow resume` hint (line 596-597) is the exact precedent for D-03's "make recovery verbs discoverable from a stuck state" — extend this `if` block (or add a sibling one) to also hint `advance` where applicable, and add a new progress line (e.g., elapsed time in current stage, or `N of M` if stage ordering is known) alongside the existing `stage:`/`liveness:` lines, using the same `println!` idiom.

---

### 21b — `crates/devflow-cli/src/commands.rs` (doctor planning-doc reconciliation)

**Analog — detect-and-report `PhaseFinding` shape, never auto-correct** (`commands.rs:1543-1566`):
```rust
pub(crate) struct PhaseFinding {
    pub(crate) phase: u32,
    pub(crate) severity: Severity,
    pub(crate) detail: String,
    pub(crate) repair: Option<String>,
}

/// `gate_pending` is set but no gate file is open for this phase — the gate
/// answer path is stuck. `doctor` only reports this; it never repairs it
/// (T-18-02).
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
21b's new check is a pure function returning `Option<PhaseFinding>` (or a new sibling `PlanningDocFinding` type per RESEARCH Open Question #1 — since planning-doc claims about *shipped* phases have no corresponding active `PhaseFacts`/`state-NN.json`). For D-04 (detection-only), set `repair: None` — there is nothing this check can auto-fix.

**Analog — single-JSON-document composition** (`commands.rs:1858-1871`, `doctor_json_body` — THE function to extend, per D-05):
```rust
/// Compose `doctor --json`'s single JSON document (WR-01, 18-fix). ...
/// There is now exactly one top-level value: `{"environment": [...], "reconciliation": [...]}`.
fn doctor_json_body(checks: &[Check], facts: &[PhaseFacts]) -> serde_json::Value {
    serde_json::json!({
        "environment": checks_json_value(checks),
        "reconciliation": render_reconciliation_json(facts),
    })
}
```
Add a third key, e.g. `"planning_doc_staleness": render_planning_doc_findings_json(&doc_findings)` — do NOT fork a second top-level array (WR-01 regression class explicitly named in the comment above this function).

**Git-tag existence/reachability helper (no direct analog exists yet — nearest idiom is `run_git_stdout`, `staleness.rs:81-91`):**
```rust
pub(crate) fn run_git_stdout(project_root: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).to_string())
}
```
21b's tag lookup (`git rev-parse --verify refs/tags/<tag>` + `git merge-base --is-ancestor <tag> <base>`) should follow this exact argv-array `Command::new("git").args([...])` idiom — never build a shell string. Note the `v`-prefix normalization gap RESEARCH flagged: `ROADMAP.md`/`STATE.md` cells are bare (`1.7.0`), tags are `v1.7.0`.

**Error/noise-avoidance pattern (Pitfall #2 — mandatory):** Scope the check to rows matching `^v?\d+\.\d+\.\d+$` only; skip ranges/em-dashes; downgrade pre-v1.5.0 mismatches to informational/Warn severity, not `Problem` — otherwise the check floods `doctor` with a dozen+ false positives on every run (verified: Phase 9/11 both claim `1.2.0`; no `v1.0.0` tag exists for Phases 6/7's claim).

---

### 21c — `crates/devflow-cli/src/parallel.rs` (sequentagent second-process record)

**Analog — the exact comment establishing the constraint** (`parallel.rs:150-198`, `run_agent_blocking`):
```rust
fn run_agent_blocking(
    project_root: &Path,
    phase: u32,
    agent: AgentKind,
    workdir: &Path,
) -> Result<Option<agent_result::AgentResult>, CliError> {
    // ...
    // Synthetic, never-persisted state: the monitor only reads project_root,
    // phase, and worktree_path from it — sequentagent does not participate
    // in the stage machine.
    let mut state = State::new(phase, agent, Mode::Auto, project_root.to_path_buf());
    state.stage = Stage::Code;
    if workdir != project_root {
        state.worktree_path = Some(workdir.to_path_buf());
    }
    let monitor_pid =
        monitor::spawn_monitor_no_advance(&state, program, &args, &adapter.extra_env())
            .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    println!("launched {} (monitor pid {monitor_pid}) in {}", adapter.name(), workdir.display());
    println!("  watch live: devflow logs -f --phase {phase} [--stderr]");
    let exit_code = monitor::wait_for_agent_exit(project_root, phase, monitor_pid)
        .map_err(|err| CliError::Message(format!("agent run did not complete: {err}")))?;
    // ...
}
```
**DO NOT** route the new record through `State`/`save_state` — already investigated and rejected in Phase 19 (D-14, the comment above `run_agent_blocking` at `:4-5` reiterates it). `run_agent_blocking` is called twice inside `sequentagent` (`:278` onward) — once per agent, both writing to the SAME `phase-NN-agent-pid`/stdout/exit paths (keyed only by `(project_root, phase)`, no agent-slot dimension — Pitfall #4).

**Analog — path-naming convention to extend** (`crates/devflow-core/src/agent_result.rs:877-896`):
```rust
pub fn stdout_path(project_root: &Path, phase: u32) -> PathBuf { /* ... */ }
pub fn stderr_path(project_root: &Path, phase: u32) -> PathBuf { /* ... */ }
pub fn exit_code_path(project_root: &Path, phase: u32) -> PathBuf { /* ... */ }
pub fn agent_pid_path(project_root: &Path, phase: u32) -> PathBuf { /* ... */ }
```
21c's new record should follow this exact `fn xxx_path(project_root: &Path, phase: u32[, agent_slot]) -> PathBuf` naming convention — the plan's first task (per D-06) is defining the data model (e.g. `sequentagent_slot_path(project_root, phase) -> PathBuf` writing `"A"`/`"B"` or an `AgentKind`+slot enum), then having `run_agent_blocking`/`sequentagent` write it and `status`/`doctor` read it via the same `agent_pid_from_file`-style helper (`commands.rs:888-891`):
```rust
fn agent_pid_from_file(project_root: &Path, phase: u32) -> Option<u32> {
    let path = agent_result::agent_pid_path(project_root, phase);
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}
```

---

### 21d — `crates/devflow-cli/src/staleness.rs` (content-aware ancestry arm)

**Analog — the dirty-tree arm's content-awareness (17-10 precedent, mirror verbatim), `staleness.rs:106-157`:**
```rust
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

fn affects_compiled_binary(rel_path: &str) -> bool {
    const BUILD_AFFECTING_FILES: [&str; 4] = ["Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml"];
    rel_path.ends_with(".rs")
        || BUILD_AFFECTING_FILES.iter().any(|name| rel_path == *name || rel_path.ends_with(&format!("/{name}")))
}
```

**Target — the exact function/branch to change** (`staleness.rs:45-76`, `embedded_commit_is_stale`):
```rust
fn embedded_commit_is_stale(execution_root: &Path, embedded_commit: &str) -> Staleness {
    if embedded_commit.is_empty() {
        return Staleness::Indeterminate;
    }
    let output = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", embedded_commit, "HEAD"])
        .current_dir(execution_root)
        .output();
    match output.map(|o| o.status.code()) {
        Ok(Some(0)) => match run_git_stdout(execution_root, &["rev-parse", "HEAD"]) {
            Some(head) if head.trim() == embedded_commit.trim() => Staleness::Fresh,
            Some(_) => Staleness::Stale,          // <-- THIS arm is the one D-07 narrows
            None => Staleness::Indeterminate,
        },
        Ok(Some(1)) => { /* Ahead/Stale reverse-probe — DO NOT TOUCH per D-07 */ }
        _ => Staleness::Indeterminate,
    }
}
```
Only the `Some(head) if head.trim() != embedded_commit.trim() => Staleness::Stale` line changes. New helper (sketch, following `run_git_stdout`'s idiom exactly):
```rust
fn ancestry_range_affects_build(execution_root: &Path, embedded_commit: &str) -> bool {
    run_git_stdout(execution_root, &["diff", "--name-only", embedded_commit, "HEAD"])
        .map(|out| out.lines().any(affects_compiled_binary))
        .unwrap_or(true) // fail toward Stale — never a false Fresh on git error
}
```
Then: `Some(_) => if ancestry_range_affects_build(execution_root, embedded_commit) { Staleness::Stale } else { Staleness::Fresh }`.

**Block-message wording fix** (`staleness.rs:298-302`, `enforce_build_staleness`):
```rust
let message = format!(
    "self-dogfood stale build blocked for stage {}: this devflow binary's \
     embedded commit is not an ancestor of {}'s current HEAD (or its tracked \
     source is newer than the build) — rebuild devflow before driving its own \
     workspace (D-18; the Phase 16 false-evidence incident){}",
    state.stage, execution_root.display(), /* ... */
);
```
D-07 flags "is not an ancestor" wording as needing a fix — since after this change, the block condition is "ancestor but a build-affecting file changed," not "not an ancestor." Reword to something like "...embedded commit's descendants touch a build-relevant file (or is not an ancestor at all)...".

**Existing regression test that MUST be edited, not left as-is** (`staleness.rs:742-811`, `wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks`):
```rust
// Second commit on top: an unrelated NEW file — no modifications to
// already-committed files, so the tree stays clean.
std::fs::write(root.join("b.txt"), "two").unwrap();
git(&["add", "."]);
git(&["commit", "-q", "-m", "unrelated follow-up"]);
// ...
assert_eq!(embedded_commit_is_stale(root, &embedded_commit), Staleness::Stale);
```
`b.txt` is not `.rs`/build-affecting, so after 21d's fix this fixture would (correctly) become `Fresh`, breaking this assertion. **Fix:** change `b.txt` to a `.rs` file (e.g. `root.join("src/main.rs")`, or reuse `a.txt`→`main.rs`), preserving the test's original intent (a genuine code change after build must still hard-block). Add two NEW sibling tests: (1) docs-only range → `Fresh` (the actual 999.29 repro), (2) mixed range (docs + `.rs`) → still `Stale` (explicitly mandated by D-07).

---

### 21e (optional stretch) — `crates/devflow-core/src/ship.rs` (`ChangelogAppend` real content)

**Target** (`ship.rs:428-431`, `prepend_changelog`):
```rust
pub fn prepend_changelog(existing: &str, version: &str, date: &str) -> String {
    // ...
    let entry = format!("## {version} — {date}\n\n- Released phase via DevFlow.\n");
    // ...
}
```
Existing test coverage to extend: `prepend_changelog_creates_header_when_empty` (:644), `prepend_changelog_inserts_after_header` (:651). D-08 requires the planner to explicitly choose the content source (SUMMARY.md extraction vs plan-diff) before implementing — this research does not mandate one.

---

## Shared Patterns

### Detect-and-report, never auto-correct (governs 21b, D-04)
**Source:** `crates/devflow-cli/src/commands.rs:1553` (`check_gate_pending_without_gate`) — `repair: Some(...)` is only used when there IS a safe corrective command to suggest; for 21b, `repair: None` since there is nothing to auto-fix in prose docs.
**Apply to:** 21b's new planning-doc `Check`/finding.

### Single-JSON-document composition (governs 21b, D-05)
**Source:** `crates/devflow-cli/src/commands.rs:1866` (`doctor_json_body`).
**Apply to:** Any new `doctor --json` key — add a new top-level key, never a second array.

### Git shelling via argv array, never shell string (governs 21b, 21d)
**Source:** `crates/devflow-cli/src/staleness.rs:81` (`run_git_stdout`) — `Command::new("git").args([...]).current_dir(...)`.
**Apply to:** Every new git invocation in 21b (tag existence/reachability) and 21d (`git diff --name-only`). Never build an interpolated shell string.

### Path-free event payloads (WR-02 discipline)
**Source:** `crates/devflow-cli/src/staleness.rs:313-323` (comment above `events::emit` call in `enforce_build_staleness`) — absolute paths/OS usernames must never reach `.devflow/events.jsonl`.
**Apply to:** Any new event emitted by 21b/21c.

### Reuse `affects_compiled_binary` verbatim, never fork (governs 21d, D-07 mandate)
**Source:** `crates/devflow-cli/src/staleness.rs:146-157`.
**Apply to:** 21d's new `ancestry_range_affects_build` helper — call the existing predicate, do not reimplement file-extension matching.

## No Analog Found

None. All five units are targeted edits inside files that already contain a directly-analogous function (same file, adjacent role) — confirmed by direct source read this session. No new files/modules are created by this phase.

## Metadata

**Analog search scope:** `crates/devflow-cli/src/{commands,main,parallel,staleness}.rs`, `crates/devflow-core/src/{ship,agent_result,gates}.rs` — all read directly, no Glob/Grep-only inference.
**Files scanned:** 7 (all listed above), plus `.planning/phases/21-*/21-{CONTEXT,RESEARCH}.md`.
**Pattern extraction date:** 2026-07-23. All line numbers verified live against current HEAD (not copied from RESEARCH.md without re-check) — every anchor RESEARCH.md cited was independently re-confirmed and matches exactly.
