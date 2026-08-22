# Phase 16: Pipeline Reliability Hardening - Pattern Map

**Mapped:** 2026-07-17
**Files analyzed:** 12 (9 new/extended core modules + main.rs call sites + config.rs + Cargo.toml)
**Analogs found:** 12 / 12 (all items extend existing modules in-place — this phase has almost
no brand-new architectural shape; RESEARCH.md's own "Recommended Project Structure" IS the
file list, and every new module has a close sibling already in the codebase to copy idiom from)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|--------------------|------|-----------|-----------------|----------------|
| `crates/devflow-core/src/hooks.rs` (fix 16k: wire Merge hook) | service (hook dispatch) | event-driven | itself — `hooks_after_ship()`/`Hook` enum/`branch_cleanup` already in file | exact (in-place fix) |
| `crates/devflow-core/src/verify.rs` (NEW, 16a) | service | request-response (shell out + check exit code) | `gates.rs::run_notify_command` (shells out via `sh -c`, env-passes untrusted context, fail-soft) | role-match |
| `crates/devflow-core/src/agent_result.rs` (extend, 16b: `cleanup_phase_files` → `archive_phase_files`) | service | file-I/O | itself — `stdout_path`/`exit_code_path`/`cleanup_phase_files` already in file | exact (in-place fix) |
| `crates/devflow-core/src/doc_check.rs` (NEW, 16c/16i) | utility / checker | batch (scan docs/source, cross-reference) | `prompt.rs`'s `#[cfg(test)] mod tests` snapshot-assertion idiom (string-contains assertions over a generated artifact) | role-match |
| `crates/devflow-core/src/prompt.rs` (extend `ship_stage_prompt`, 16d/16e) | service (prompt composition) | transform (string building) | itself — `ship_stage_prompt`/`validate_stage_prompt`/`idempotent_stage_prompt` already establish the per-stage dedicated-prompt idiom | exact (in-place fix) |
| `crates/devflow-core/src/config.rs` (extend, D-03: `devflow.toml` loader) | config | file-I/O (parse + precedence merge) | itself — `GitFlowConfig` (`Default`-only-constructor idiom); precedence pattern mirrors `gates.rs::fire_gate_notify`'s env-var-first read | role-match |
| `crates/devflow-cli/src/main.rs::project_root` (fix 16f: walk-up resolver) | utility (CLI arg resolution) | transform | itself — current bare-canonicalize function; git's own `.git`-search semantics (`git rev-parse --show-toplevel`) is the conceptual analog, no in-repo code equivalent exists | role-match |
| `crates/devflow-cli/src/main.rs::GateCmd::Approve/Reject` (fix 16g: positional footgun) | CLI schema (clap derive) | request-response | itself — sibling `Command::Start`/`Advance` variants using `#[arg(default_value = ".")]` trailing positional `project: PathBuf` | exact (in-place schema fix) |
| `crates/devflow-core/src/workflow.rs::migrate_legacy_state` (fix 16g: WARN hint) | service | transform | itself — existing `warn!(...)` call site | exact (in-place fix) |
| `crates/devflow-core/src/history.rs` (NEW, 16h) | service | transform (correlate two read-only stores) | `events.rs::last_events_by_phase` (single-pass read+parse+fold over a JSONL log) | role-match |
| `crates/devflow-core/src/gates.rs` (extend `fire_gate_notify`, 16j: persistent status indicator) | service | event-driven | itself — `fire_gate_notify`/`run_notify_command` already in file | exact (in-place fix) |
| `crates/devflow-cli/src/main.rs::status` (extend, 16j: escalating banner) | CLI command handler | request-response | itself — existing `status` rendering of `open branches`/gate-pending state (main.rs ~2080) | exact (in-place fix) |

## Pattern Assignments

### `crates/devflow-core/src/hooks.rs` (16k — wire Merge hook into terminal path)

**Analog:** itself (in-place fix); merge primitive already implemented in `git.rs`

**Current buggy state** (`hooks.rs:83-86`):
```rust
/// Hooks that fire after Ship completes (the workflow's terminal transition).
pub fn hooks_after_ship() -> Vec<Hook> {
    vec![Hook::VersionBump, Hook::BranchCleanup]   // <-- no merge hook
}
```

**Hook enum + dispatch pattern to extend** (`hooks.rs:16-69`):
```rust
pub enum Hook {
    BranchCreate,
    BranchCleanup,
    DocsUpdate,
    ChangelogAppend,
    VersionBump,
    // NEW: Merge,
}

impl Hook {
    pub fn run(&self, ctx: &HookContext) -> Result<(), HookError> {
        match self {
            Hook::BranchCreate => branch_create(ctx),
            Hook::BranchCleanup => branch_cleanup(ctx),
            Hook::DocsUpdate => docs_update(ctx),
            Hook::ChangelogAppend => changelog_append(ctx),
            Hook::VersionBump => version_bump(ctx),
            // Hook::Merge => merge_feature(ctx),
        }
    }
}
```

**Existing merge primitive to call, already unit-tested** (`git.rs:71-78`):
```rust
/// Merge a feature branch into develop and delete it.
pub fn feature_finish(&self, phase: u32) -> Result<String, GitError> {
    let branch = format!("{}phase-{:02}", self.config.feature_prefix, phase);
    info!("finishing feature branch: {branch}");
    self.git(["checkout", &self.config.develop])?;
    self.git(["merge", "--no-ff", &branch])?;
    self.git(["branch", "-d", &branch])?;
    Ok(branch)
}
```

**Ordering constraint (Pitfall 2 in RESEARCH.md):** the new Merge hook MUST run FIRST in the
batch — `vec![Hook::Merge, Hook::VersionBump, Hook::BranchCleanup]` — so `VersionBump`'s
`compute_version` (`version.rs:142`, counts `git rev-list --count` since last tag) runs
against the post-merge `develop` HEAD, not the worktree's private branch tip. This mirrors
the existing `hooks_for_transition(Validate, Ship)` ordering (`[DocsUpdate, ChangelogAppend]`
— docs before changelog before any version stamp).

**Fail-soft hook body pattern to copy** (`hooks.rs:95-115`, `branch_cleanup`):
```rust
fn branch_cleanup(ctx: &HookContext) -> Result<(), HookError> {
    let git = GitFlow::new(&ctx.project_root);
    let branch = format!("{}phase-{:02}", ctx.git_flow.feature_prefix, ctx.phase);
    if git.branch_exists(&branch) {
        match git.delete_branch(&branch, false) {
            Ok(()) => info!("BranchCleanup: deleted {branch}"),
            Err(err) => {
                let message = err.to_string();
                if message.contains("not fully merged") || message.contains("not yet merged") {
                    warn!("BranchCleanup: feature branch {branch} is not merged yet — left in place");
                } else {
                    warn!("BranchCleanup: could not delete {branch}: {err}");
                }
            }
        }
    }
    Ok(())
}
```
Per RESEARCH.md Pitfall 1, the new Merge hook's event emission should distinguish "ran
without error" from "the operation's intended effect happened" — e.g. emit a
`"merged": true/false` field alongside `"ok"` in the `hook_run` event (see `main.rs:975-983`
below), not conflate the two the way `BranchCleanup`'s current `Ok(())`-on-no-op does.

**Test pattern to extend** (`hooks.rs:247-253`, currently asserts the buggy list — must be
updated, not just added to):
```rust
#[test]
fn after_ship_runs_version_and_cleanup() {
    assert_eq!(
        hooks_after_ship(),
        vec![Hook::VersionBump, Hook::BranchCleanup]   // update to include Hook::Merge first
    );
}
```
Idempotency test (Open Question 1 in RESEARCH.md): add a case exercising `Hook::Merge` when
the branch is ALREADY merged (or absent) — must be a safe no-op, not an error, mirroring
`branch_cleanup_is_fail_soft_when_branch_absent` (`hooks.rs:294-302`).

**Call site — hook batch dispatch + event emission** (`main.rs:929-985`, `run_checkout_hooks`):
```rust
for hook in batch {
    let ctx = HookContext { phase: state.phase, project_root: project_root.to_path_buf(), stage, git_flow: git_flow.clone() };
    let outcome = hook.run(&ctx);
    if let Err(ref err) = outcome {
        println!("warning: hook {hook:?} failed: {err}");
    }
    events::emit(project_root, state.phase, "hook_run",
        serde_json::json!({ "hook": format!("{hook:?}"), "ok": outcome.is_ok() }));
}
```
`finish_workflow` (`main.rs:1055-1068`) is the terminal caller — no changes needed there
beyond `hooks_after_ship()`'s new return value; `run_checkout_hooks` already iterates in
list order and emits one `hook_run` event per hook.

---

### `crates/devflow-core/src/verify.rs` (NEW — 16a external post-condition verification)

**Analog:** `gates.rs::run_notify_command` (`gates.rs:296-319`) — nearest existing
"shell out to an operator/plan-declared command and check only its exit status, fail-soft"
pattern in the codebase.

**Imports pattern to copy** (`gates.rs:1-20` header, adapted):
```rust
use std::process::Command;
use tracing::{debug, warn};
```

**Core shell-out + fail-soft pattern to copy** (`gates.rs:296-319`):
```rust
fn run_notify_command(cmd: &str, phase: u32, stage: Stage, context: &str, unexpected: bool) {
    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .env("DEVFLOW_GATE_PHASE", phase.to_string())
        // ... more env(), NEVER interpolated into the command string
        .output();
    match output {
        Ok(out) if out.status.success() => debug!("gate notify hook ran successfully"),
        Ok(out) => warn!("gate notify hook exited with status {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr)),
        Err(err) => warn!("gate notify hook could not be spawned: {err}"),
    }
}
```
Security note from RESEARCH.md's Security Domain section: the verification command's source
MUST be the PLAN.md (operator-authored, already-trusted per this project's existing threat
model) — never agent stdout or any other runtime-produced string. This should be asserted as
a test, not just a convention, per RESEARCH.md's explicit recommendation.

**Layer ordering to hook into** — `evaluate_agent_result` (`agent_result.rs:627-644`):
```rust
pub fn evaluate_agent_result(project_root: &Path, state: &State, git_flow: &GitFlowConfig) -> Result<AgentResult, ResultError> {
    // Layer 1: DEVFLOW_RESULT marker (authoritative)
    if let Some(result) = evaluate_layer1(project_root, state.phase) { return Ok(result); }
    // Layer 2: Exit code + commit gate
    if let Some(result) = evaluate_layer2(project_root, state.phase, git_flow, state.stage)? { return Ok(result); }
    // Layer 3: Process existence + commits
    evaluate_layer3(project_root, state.phase, git_flow)
}
```
16a's "Layer 0" must run BEFORE Layer 1 and OUTRANK it for plans that declare an external
verification command — same three-tier `if let Some(...) return` cascade shape, prepended.

**AgentResult struct to construct/return** (`agent_result.rs:16` + surrounding `AgentStatus`
variants) — read `agent_result.rs:1-60` at implementation time for the exact field list
(`status`, `exit_code`, `reason`, `commits`, `summary`, `verdict`); Layer 0's result should
populate `reason` with the probe command and its outcome, matching the descriptive-`reason`
convention already used in `evaluate_layer2` (`agent_result.rs:565-580`).

**Command example from RESEARCH.md** (registry-probe shape, illustrative):
```rust
fn run_external_verification(cmd: &str, project_root: &Path) -> Result<bool, VerifyError> {
    let output = std::process::Command::new("sh").arg("-c").arg(cmd)
        .current_dir(project_root).output()?;
    Ok(output.status.success())
}
```

---

### `crates/devflow-core/src/agent_result.rs` (16b — retained capture history)

**Analog:** itself — extend `cleanup_phase_files`, keep `stdout_path`/`exit_code_path`/
`agent_pid_path` unchanged.

**Current function being replaced/extended** (`agent_result.rs:646-677`):
```rust
fn devflow_dir(project_root: &Path) -> PathBuf {
    project_root.join(".devflow")
}
pub fn stdout_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-stdout", phase))
}
pub fn exit_code_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-exit", phase))
}
pub fn agent_pid_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-agent-pid", phase))
}
/// Clean up old stdout, exit code, and agent-pid files for a phase before starting.
pub fn cleanup_phase_files(project_root: &Path, phase: u32) {
    let _ = std::fs::remove_file(stdout_path(project_root, phase));
    let _ = std::fs::remove_file(exit_code_path(project_root, phase));
    let _ = std::fs::remove_file(agent_pid_path(project_root, phase));
}
```
Rename to `archive_phase_files` and rotate into `.devflow/history/phase-NN/` instead of
`remove_file`, per RESEARCH.md's Pattern 3 illustrative shape (uses `std::fs::rename` +
`prune_history` bounded by the new `devflow.toml` retention knob). Same call site
(`launch_stage`, `main.rs:634`) — only the function body changes.

**Existing test to update, not just add to** (`agent_result.rs` test module — grep for
`cleanup_removes_phase_files` per RESEARCH.md's Phase-Requirements table; the current test
asserts deletion and must be changed to assert retention-with-bounded-pruning instead).

---

### `crates/devflow-core/src/doc_check.rs` (NEW — 16c/16i deterministic checkers)

**Analog:** `prompt.rs`'s `#[cfg(test)] mod tests` — string/structural assertion idiom over
a generated artifact, the closest existing "compare expected structure against reality"
shape in this codebase (no existing grep-cross-reference tool exists per RESEARCH.md).

**Test-module idiom to copy** (`prompt.rs:175-193`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_stage_prompt_carries_its_gsd_command_and_marker() {
        let cases = [ /* ... */ ];
        for (stage, command) in cases {
            let prompt = stage_prompt(stage, 11);
            assert!(prompt.contains(command), "{stage} prompt missing {command}");
        }
    }
}
```
D-05 locks 16c/16i as `#[test]` functions, not a new binary/hook — this table-driven
assertion shape (iterate a list of expected facts, assert each against a generated/scanned
artifact) is the pattern to follow for both the existence-checker (16c layer 1) and the
`.gitignore` invariant (16i).

**Path-enumeration-from-source pattern (16i)** — per RESEARCH.md Pitfall 4, do NOT
hardcode a `Vec<&str>` of paths; call each module's own path-constructor function:
```rust
// Illustrative — call real constructors, not a literal list:
let paths = [
    events::events_path(root),
    agent_result::stdout_path(root, 0),   // pattern-generalize the {:02} phase segment
    agent_result::exit_code_path(root, 0),
    agent_result::agent_pid_path(root, 0),
    // gates::gates_dir(root), workflow::state_path(root, ...), lock::lock_path(root), etc.
];
```
Existing path-constructor functions to enumerate from: `events::events_path` (`events.rs:26`),
`agent_result::stdout_path`/`exit_code_path`/`agent_pid_path`/`stderr_path`
(`agent_result.rs:652-670`) — grep `gates.rs`, `workflow.rs`, `lock.rs`, `ship.rs`,
`monitor.rs` at implementation time for the remaining constructors per the canonical
reference's enumeration list.

**Allowlist file (D-07):** no existing analog in this codebase for a checked-in exceptions
file; model it as a plain text/TOML file with one entry per line, each carrying a required
`# reason: ...` comment, loaded the same way `config.rs` will load `devflow.toml` (D-03) —
i.e. reuse the new TOML-loading idiom being introduced for config, don't invent a second
parser.

---

### `crates/devflow-core/src/prompt.rs` (16d/16e — Ship review angle list + fan-out)

**Analog:** itself — `ship_stage_prompt` (`prompt.rs:52-72`) is the exact function to extend.

**Current function to extend**:
```rust
fn ship_stage_prompt(phase: u32) -> String {
    let code_review = format!("/gsd-code-review {phase}");
    let ship = format!("/gsd-ship {phase}");
    format!(
        "Run the Ship stage in two steps:\n\
        \n\
        1. Run `{code_review}` (non-interactive). This writes a `REVIEW.md` \
        artifact with severity-classified findings.\n\
        2. Check `REVIEW.md` for the Critical-severity gate:\n\
        \n\
        - If `REVIEW.md` contains ANY finding at Critical severity: do NOT \
        run `{ship}` at all. ...\n\
        \n\
        {COMPLETION_PROTOCOL}"
    )
}
```
D-01/D-02 extend step 1's instruction text only — the two-step Critical-gate structure
(steps 1–2, the `review:`-prefixed failure contract) stays exactly as-is per RESEARCH.md's
Code Examples section.

**Angle-list constant pattern (from RESEARCH.md, matches this file's `const
COMPLETION_PROTOCOL` idiom at `prompt.rs:11-23`):**
```rust
const SHIP_REVIEW_ANGLES: &str = "\
- doc-accuracy cross-reference (do documented claims match source?)\n\
- security / leaked-data (does anything commit secrets, session data, telemetry?)\n\
- CI/build correctness (can a failing step still report green?)\n\
- external-state claims (does the diff assert something about state DevFlow \
  doesn't actually control — merges, tags, deletions — that isn't actually true?)\n\
- one generalist deep pass";
```

**Snapshot-test pattern to extend** (`prompt.rs:201-238`,
`ship_prompt_sequences_code_review_before_ship` +
`ship_prompt_defines_critical_gate_and_review_failed_contract`):
```rust
#[test]
fn ship_prompt_defines_critical_gate_and_review_failed_contract() {
    let prompt = stage_prompt(Stage::Ship, 13);
    assert!(prompt.contains("REVIEW.md"), "...");
    assert!(prompt.to_lowercase().contains("critical"), "...");
    assert!(prompt.contains("review:"), "...");
    assert!(prompt.contains("DEVFLOW_RESULT"));
}
```
D-02 requires the angle list itself to be snapshot-tested — add a new test in this same
module asserting `prompt.contains(...)` for each of the four incident-derived angles plus
the generalist pass, and asserting the conditional parallel-subagent-or-sequential-pass
instruction text is present (capability-conditional, harness-agnostic per D-01 — must not
name "Claude Code" or "Task tool" literally in a way that breaks under Codex/OpenCode).

---

### `crates/devflow-core/src/config.rs` (D-03 — minimal `devflow.toml`)

**Analog:** itself — `GitFlowConfig`'s `Default`-only-constructor idiom (`config.rs:20-38`)
is the existing config-struct shape to extend, not replace.

**Current struct/doc-comment to update** (`config.rs:1-38`):
```rust
//! Git-flow branch model.
//!
//! DevFlow has no `.devflow.yaml` and no automation toggles — all behavior is
//! driven by CLI flags (`--mode`, `--agent`, …). The only project configuration
//! left is the git-flow branch model, and that is hardcoded to opinionated
//! constants: `main`, `develop`, and the `feature/` prefix.
```
This doc comment is explicitly what D-03 requires correcting (the phase's own canonical
reference calls this out). `GitFlowConfig` itself (main/develop/feature_prefix) stays
hardcoded and untouched — only the module-level doc comment and a NEW sibling struct for
the Phase 16 knobs (review angles override, capture-retention N, verification settings)
are added, following the same `#[derive(Debug, Clone, PartialEq, Eq)]` + `Default` shape:
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFlowConfig {
    pub main: String,
    pub develop: String,
    pub feature_prefix: String,
}
impl Default for GitFlowConfig {
    fn default() -> Self { GitFlowConfig { main: MAIN.to_string(), develop: DEVELOP.to_string(), feature_prefix: FEATURE_PREFIX.to_string() } }
}
```

**Precedence pattern to copy (env > file > default)** — `gates.rs::fire_gate_notify`
(`gates.rs:282-288`) is the existing env-var-read idiom:
```rust
pub fn fire_gate_notify(phase: u32, stage: Stage, context: &str, unexpected: bool) {
    let cmd = match std::env::var("DEVFLOW_GATE_NOTIFY_CMD") {
        Ok(cmd) if !cmd.is_empty() => cmd,
        _ => return,
    };
    run_notify_command(&cmd, phase, stage, context, unexpected);
}
```
The new `devflow.toml` loader should layer: check `DEVFLOW_*` env var first (existing
idiom, untouched per D-03) → else parse `devflow.toml` if present (new, via `toml` crate +
`serde::Deserialize`, per RESEARCH.md's explicit "Don't Hand-Roll" guidance — do not
extend `version.rs`'s narrow hand-rolled TOML value-replacement into a general parser) →
else built-in default (the existing `Default` impl shape above).

**Test pattern to copy** (`config.rs:40-51`):
```rust
#[test]
fn default_uses_hardcoded_constants() {
    let config = GitFlowConfig::default();
    assert_eq!(config.main, "main");
    ...
}
```

---

### `crates/devflow-cli/src/main.rs::project_root` (16f — shared walk-up resolver)

**Analog:** itself — the current bare-canonicalize function is the single call site every
`Command`/`GateCmd` arm goes through (confirmed: `Command::Start`, `Advance`, `Gate::List`,
`Gate::Approve`, `Gate::Reject` all call `project_root(project)?` — `main.rs:328-347`).

**Current implementation to replace** (`main.rs:2093-2104`):
```rust
fn project_root(project: PathBuf) -> Result<PathBuf, CliError> {
    if project.exists() {
        project
            .canonicalize()
            .map_err(|err| CliError::Message(format!("failed to resolve project path: {err}")))
    } else {
        Err(CliError::Message(format!(
            "project path does not exist: {}",
            project.display()
        )))
    }
}
```

**Call sites already funneled through this one function** (`main.rs:328-347`):
```rust
Command::Start { ... project } => { ... start(&project_root(project)?, ...) }
Command::Advance { project, phase } => advance(&project_root(project)?, phase),
Command::Gate { action } => match action {
    GateCmd::List { project } => gate_list(&project_root(project)?),
    GateCmd::Approve { phase, stage, note, project } => gate_respond(&project_root(project)?, phase, stage, true, note),
    GateCmd::Reject { phase, stage, ... } => ...
}
```
Confirms the "fix once, in the shared function" strategy from RESEARCH.md's Pattern 1 is
correct — every subcommand already routes through this exact function, no per-call-site
changes needed. Implement the walk-up loop from RESEARCH.md's Pattern 1 verbatim (hand-
rolled, no new dependency — the `project-root` crate is explicitly rejected in Standard
Stack as Cargo.lock-specific, wrong marker semantics).

---

### `crates/devflow-cli/src/main.rs::GateCmd` (16g — positional-arg footgun)

**Analog:** itself — the enum's own field ordering is the bug.

**Current schema causing the footgun** (`main.rs:225-265`):
```rust
enum GateCmd {
    Approve {
        phase: u32,
        #[arg(long)]
        stage: Option<Stage>,
        #[arg(long)]
        note: Option<String>,
        #[arg(default_value = ".")]
        project: PathBuf,   // <-- trailing bare positional swallows a misplaced value
    },
    ...
}
```
`devflow gate approve 15 ship` binds `"ship"` to the trailing `project` positional (clap's
documented trailing-positional-plus-subcommand ambiguity, per RESEARCH.md A4) — producing
"project path does not exist: ship" with no hint. Fix options per RESEARCH.md: make `stage`
positional-optional, drop the positional `project` in favor of `--project`, or detect the
unmatched positional equals a `Stage`-parseable string and emit a "did you mean `--stage
ship`?" hint in the `CliError::Message` path (`main.rs:281-283`, the `Message(String)`
variant is the existing free-text error channel to reuse).

---

### `crates/devflow-core/src/workflow.rs::migrate_legacy_state` (16g — WARN hint)

**Analog:** itself — locate via `rg "legacy state" crates/devflow-core/src/workflow.rs`
(around `workflow.rs:50-61` per RESEARCH.md). Extend the existing `warn!(...)` call's
message string to append a `devflow recover --clean` hint; no structural change, matches
the existing single-line `warn!` idiom already used throughout `hooks.rs`/`gates.rs`.

---

### `crates/devflow-core/src/history.rs` (NEW — 16h cross-attempt history)

**Analog:** `events.rs::last_events_by_phase` (`events.rs:75-92`) — single-pass
read+parse+fold over the JSONL log, the exact shape 16h should reuse rather than inventing
a new store (per the canonical reference's explicit instruction).

**Pattern to copy**:
```rust
pub fn last_events_by_phase(project_root: &Path) -> std::collections::HashMap<u32, serde_json::Value> {
    let mut latest = std::collections::HashMap::new();
    let Ok(contents) = std::fs::read_to_string(events_path(project_root)) else { return latest; };
    for event in contents.lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok()) {
        if let Some(phase) = event.get("phase").and_then(|p| p.as_u64()) {
            latest.insert(phase as u32, event);
        }
    }
    latest
}
```
16h needs the FULL per-phase event history (not just latest), so iterate/collect into a
`Vec` per phase instead of overwriting — same read-once-fold-in-memory shape, different
accumulator. Correlate against 16b's retained `.devflow/history/phase-NN/` capture files
and any retained `REVIEW.md` snapshots (file naming/location decided by 16b's
implementation — read that module's final path scheme before wiring 16h's correlation).

**Events schema reference** (`events.rs:1-19` module doc comment) — the JSONL schema v1
(`{"v":1,"ts":...,"phase":...,"event":...}`) that 16h must parse against; `describe()`
(`events.rs:99+`) is the existing human-readable-summary function to extend/reuse for
16h's rendered view rather than writing a second formatter.

---

### `crates/devflow-core/src/gates.rs` + `main.rs::status` (16j — verifiable notification)

**Analog:** itself — `fire_gate_notify`/`run_notify_command` (`gates.rs:275-319`) already
in file; `status`'s existing gate-pending rendering in `main.rs` (~line 2080, "open
branches" printer) is the nearest existing "render current pending state to the operator"
call site to extend with an escalating banner.

**Current fire-and-forget pattern** (`gates.rs:282-319`, already excerpted above under
16a) — RESEARCH.md's design conclusion (Open Question 2, treat as (a)) is that 16j's fix
does NOT touch this function's exit-code-only checking; it ADDS a separate, persistent
`devflow status` indicator that survives independent of whatever `DEVFLOW_GATE_NOTIFY_CMD`
reports. `fire_gate_notify` itself stays a thin wrapper — do not conflate "the notify
command ran" with "a human saw it."

**Truncation pattern to reuse, not bypass** — RESEARCH.md's Security Domain section flags
`truncate_reason` (`main.rs:849`) as the existing mitigation for agent-controlled strings
reaching a notify/status context; 16j's new persistent indicator MUST reuse this same
truncation function when rendering gate `context` in the escalating banner, not render the
raw reason verbatim in a new surface.

## Shared Patterns

### Fail-soft shell-out (applies to 16a's verify.rs AND any 16k merge-hook error path)
**Source:** `gates.rs::run_notify_command` (`gates.rs:296-319`), `hooks.rs::branch_cleanup`
(`hooks.rs:95-115`), `hooks.rs::docs_update` (`hooks.rs:117-137`)
```rust
match output {
    Ok(out) if out.status.success() => { /* log success, never propagate as hard error */ }
    Ok(out) => warn!("... exited with status {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr)),
    Err(err) => warn!("... could not be spawned: {err}"),
}
```
Every existing shell-out in this codebase uses `Command::new(...)` + `.arg()`/`.env()` with
structured args — NEVER raw string interpolation of untrusted input into a single shell
command. 16a's verification-command runner must follow this exact pattern; the command
source is trusted (PLAN.md, operator-authored), but the pattern discipline still applies.

### Event emission (applies to 16k's Merge hook, 16j's notification, 16h's history read)
**Source:** `events.rs::emit` (`events.rs:35-71`), called from `main.rs::run_checkout_hooks`
(`main.rs:975-983`) and `finish_workflow` (`main.rs:1060-1065`)
```rust
events::emit(project_root, state.phase, "hook_run",
    serde_json::json!({ "hook": format!("{hook:?}"), "ok": outcome.is_ok() }));
```
Envelope keys (`v`, `ts`, `phase`, `event`) always win over payload keys — a payload cannot
forge another phase's identity. Any new event kind (e.g. 16k's merge-hook outcome, per
Pitfall 1's recommended `"merged": true/false` field) follows this exact `emit(root, phase,
kind, json!({...}))` call shape.

### `#[test]`-as-enforcement (applies to 16c, 16i, and D-05 generally)
**Source:** every module's existing `#[cfg(test)] mod tests` block (`hooks.rs:185-303`,
`prompt.rs:175-314`, `config.rs:40-51`) — table-driven or single-assertion `#[test] fn`s
that run under plain `cargo test`, no external harness. D-05 locks 16c/16i into this exact
idiom: new `#[test] fn`s in `doc_check.rs`, not a new hook/binary/CI step.

### Precedence: env var > file > built-in default (D-03, applies to config.rs AND 16b's
retention-N AND 16a's verification settings knobs)
**Source:** `gates.rs::fire_gate_notify` (`gates.rs:282-288`) — the existing env-var-wins
idiom that D-03 explicitly says stays untouched and layers UNDER the new `devflow.toml`.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/devflow-core/src/doc_check.rs` allowlist file format | config/data file | file-I/O | No existing checked-in exceptions-file-with-required-reason-comment pattern exists in this codebase; use RESEARCH.md's D-07 description directly (simplest plain-text or TOML format, reusing the new `toml` parsing idiom being added for D-03 rather than inventing a second format/parser) |
| Escalating `devflow status` banner (16j) | CLI output formatting | request-response | No existing "escalating/persistent indicator that gets louder over time" — closest is the existing plain gate-pending line in `status`; this is genuinely new UI behavior, not a copy of an existing pattern (Claude's Discretion per CONTEXT.md — planner/implementer designs escalation semantics) |

## Metadata

**Analog search scope:** `crates/devflow-core/src/{hooks,agent_result,prompt,config,events,
gates,workflow,git,version}.rs`, `crates/devflow-cli/src/main.rs` (targeted sections:
`GateCmd`, `CliError`, `run`, `project_root`, `run_checkout_hooks`, `finish_workflow`,
`status` rendering)
**Files scanned:** 10 core modules + 1 CLI entrypoint (all read this session; no file
exceeded 2,000 lines requiring grep-then-offset strategy except `main.rs` at 3,334 lines,
handled via targeted `Grep` + non-overlapping `Read` ranges)
**Pattern extraction date:** 2026-07-17
