# Phase 17: Pipeline Dogfood Follow-Up - Pattern Map

**Mapped:** 2026-07-18
**Files analyzed:** 8 (5 modified existing, 1 new `build.rs`, 1 new pure-policy module [location: Claude's discretion], 1 event-schema extension folded into existing `events.rs` call sites)
**Analogs found:** 8 / 8 (all in-tree — this phase is pure extension, no cross-project analogs needed per RESEARCH.md)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|--------------------|------|-----------|-----------------|----------------|
| `crates/devflow-core/src/agent_result.rs` (`evaluate_layer0`, `evaluate_layer2`, `evaluate_layer3`, `AgentStatus`) | service (pure evaluation) | transform (classify agent output → typed status) | itself — extend existing four-layer cascade in place | exact (same file, existing pattern) |
| `crates/devflow-core/src/outcome_policy.rs` (NEW, D-12) or folded into `agent_result.rs` | service (pure policy) | transform (typed outcome → typed action) | `crates/devflow-cli/src/main.rs:1164-1192` `prepare_loop_back_to_code` (the 13-01 extraction precedent named explicitly in CONTEXT D-12) | exact — CONTEXT.md names this as the pattern to follow |
| `crates/devflow-core/src/agents/mod.rs` (`AgentAdapter::preflight()` new default method) | middleware (trait hook) | request-response (pre-launch gate) | `extra_env()` default impl, same file, lines 39-41 | exact |
| `crates/devflow-cli/src/main.rs` (`advance()` dispatch, D-01/D-02/D-06) | controller | request-response (stage-advance decision) | itself — extend existing `advance()` match arms (lines 854-887) | exact |
| `crates/devflow-cli/src/main.rs` (`run_preflight`, NEW, called from `launch_stage`) | controller (pre-flight gate) | request-response | `handle_stage_failure` (981-1009) + `run_gate`/`GateAction` dispatch shape; also mirrors `ensure_agent_binary` (680-687) as "the only preflight today" | role-match (gate+notify shape) / exact (existing preflight precedent) |
| `crates/devflow-cli/build.rs` (NEW, D-20) | config (build script) | batch (compile-time git shell-out → env vars) | no in-tree analog (first `build.rs` in workspace) — use RESEARCH.md's cited cargo idiom | no analog — see "No Analog Found" |
| `crates/devflow-cli/src/main.rs` (`workflow_started` emit extension, D-21) | controller (event emission) | event-driven | `events::emit(...)` call at `main.rs:605-614` (the exact site being extended) + `prepare_loop_back_to_code`'s `events::emit` at 1178-1186 as a second same-crate emit-shape example | exact |
| `crates/devflow-core/src/events.rs` (no structural change — payload fields only) | service (log writer) | event-driven | itself — `pub fn emit` at `events.rs:35` (append-only, envelope-keys-win) | exact |

## Pattern Assignments

### `crates/devflow-core/src/agent_result.rs` — Layer 0/2/3 edits (17a, D-02/D-05/D-06)

**Analog:** itself (existing four-layer cascade)

**Imports pattern** (lines 1-13):
```rust
use crate::config::GitFlowConfig;
use crate::stage::Stage;
use crate::state::State;
use std::path::{Path, PathBuf};
```

**Enum pattern to extend** (`AgentStatus`, lines 38-50) — add `ResourceKilled`, `AgentUnavailable` here, and whatever D-02 typed replacement(s) for Layer 3's blanket `Unknown` require:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Success,
    Failed,
    RateLimited,
    Unknown,
}
```
CAUTION (Common Pitfall 1 in RESEARCH.md, verified): `#[serde(rename_all = "lowercase")]` does NOT insert word separators — `ResourceKilled` serializes to `"resourcekilled"` not `"resource_killed"`. New multi-word variants need explicit `#[serde(rename = "resource_killed")]` etc. Same trap applies to `main.rs:848`'s `format!("{:?}", status).to_ascii_lowercase()` used for `advance_evaluated`'s wire field — that `Debug`-based formatter must NOT be reused as-is for multi-word variants; write a dedicated match/formatter instead.

**Layer 0 — the D-05 extension target** (lines 627-690): currently gated to `state.stage != Stage::Code` (line 638) and only ever returns `Failed` or `None` (never `Success`). D-05 requires: (a) drop the `Stage::Code` restriction, (b) add a branch where all declared probes pass → `Some(AgentResult { status: AgentStatus::Success, .. })`. Preserve the `TRUST_EXTERNAL_VERIFY_ENV` approval-mismatch checks (lines 644-678) byte-for-byte — those are the security property D-05 says must not be relaxed.
```rust
fn evaluate_layer0(
    project_root: &Path,
    state: &State,
    approved_commands: Option<&[String]>,
) -> Option<AgentResult> {
    if state.stage != Stage::Code || !crate::config::external_verify_enabled(project_root) {
        return None;
    }
    // ... approval-mismatch / not-approved / command-changed branches unchanged ...
    commands
        .into_iter()
        .find(|command| !crate::verify::run_external_verification(command, execution_root))
        .map(|command| AgentResult { status: AgentStatus::Failed, .. })
    // D-05: when `.find()` returns None (all commands passed) and at least one
    // was declared, this must become `Some(AgentResult { status: Success, .. })`
    // instead of falling through to `None`/defer-to-cascade.
}
```

**Layer 2 — decision-matrix doc comment, DO NOT TOUCH the Define/Validate/Ship zero-commit branch** (lines 500-514, 556-557):
```rust
let commit_gated = matches!(stage, Stage::Plan | Stage::Code);
let no_work_done = commit_gated && commits == 0;
```
RESEARCH.md Pitfall 2 (verified): this is deliberate normal-operation behavior. D-03's "no declaration → ambiguous → fail" targets Layer 3 only, not this branch.

**Layer 3 — the D-01/D-02 fix locus** (lines 592-625): currently always returns `AgentStatus::Unknown` with a `reason` string distinguishing commits>0 vs commits==0 in prose only. D-02 requires splitting this into typed outcomes (structurally, not just prose) so `advance()`'s match becomes exhaustive over the new variants:
```rust
pub fn evaluate_layer3(
    project_root: &Path,
    phase: u32,
    git_flow: &GitFlowConfig,
) -> Result<AgentResult, ResultError> {
    // ... commit count via `git rev-list --count` (unchanged git-shelling idiom) ...
    Ok(AgentResult {
        status: AgentStatus::Unknown, // ← D-02: replace with typed variant(s);
                                        //   commits==0 sub-case is D-03 case 3
                                        //   (ambiguous → treat as failure)
        exit_code: None,
        reason: if commits > 0 { Some(format!("unverified — ...")) }
                else { Some("no work detected — ...".into()) },
        commits: Some(commits),
        summary: None,
        verdict: None,
    })
}
```

**Layer 2 — new exit-code branches for D-07** (inside the existing `exit_code` match region, lines 559-564): add `exit_code == 137 → ResourceKilled`, `exit_code == 127 → AgentUnavailable`, ahead of the existing `exit_code != 0 → Failed` catch-all. Per RESEARCH.md Pitfall 1a: the exit code is ALREADY a plain `i32` parsed from a shell-written text file (line 528-529: `.trim().parse().unwrap_or(-1)`) — no `ExitStatusExt::signal()` needed anywhere.

**Cascade wiring — unchanged shape** (lines 702-725):
```rust
fn evaluate_agent_result_inner(
    project_root: &Path,
    state: &State,
    git_flow: &GitFlowConfig,
    approved_commands: Option<&[String]>,
) -> Result<AgentResult, ResultError> {
    if let Some(result) = evaluate_layer0(project_root, state, approved_commands) {
        return Ok(result);
    }
    if let Some(result) = evaluate_layer1(project_root, state.phase) {
        return Ok(result);
    }
    if let Some(result) = evaluate_layer2(project_root, state.phase, git_flow, state.stage)? {
        return Ok(result);
    }
    evaluate_layer3(project_root, state.phase, git_flow)
}
```

---

### `crates/devflow-core/src/outcome_policy.rs` (NEW, D-11/D-12) — pure policy function

**Analog:** `crates/devflow-cli/src/main.rs:1159-1192` `prepare_loop_back_to_code` — the explicit 13-01 extraction precedent CONTEXT.md D-12 names.

**Extraction pattern to copy** (the "split the state-mutating/IO half from the pure decision half" shape — note `prepare_loop_back_to_code` itself is NOT pure (it does I/O), but its DOC COMMENT is the precedent to imitate for *why* to split, and its caller shape (`loop_back_to_code` at line ~1154 thin-wraps it) is the shape D-12's `advance()` call site should mirror):
```rust
/// The state-mutating half of `loop_back_to_code`, split out so it's
/// unit-testable without spawning a real agent process (`launch_stage`
/// invokes the actual configured agent CLI).
fn prepare_loop_back_to_code(
    project_root: &Path,
    state: &mut State,
    fix: FixType,
) -> Result<String, CliError> { /* ... */ }
```

**Illustrative shape from RESEARCH.md** (Pattern 2, exhaustive match, no I/O, no `CliError` — this is the actual target shape for the new module):
```rust
pub fn decide_action(stage: Stage, outcome: AgentStatus, declared_probe: bool) -> Action {
    match (stage, outcome) {
        (_, AgentStatus::Success) => Action::Advance,
        (_, AgentStatus::RateLimited) => Action::AutoResume, // D-09
        (_, AgentStatus::ResourceKilled) => Action::GateInfra, // D-08 separate counter
        (_, AgentStatus::AgentUnavailable) => Action::GateInfra,
        (_, AgentStatus::Failed) => Action::GateOrAbort,
        // Unknown/its typed replacement(s): NEVER Advance (D-01/D-06)
    }
}
```
Note: `Action` variants, exact `AgentStatus` variant set, and module location (new file vs. folded into `agent_result.rs`) are Claude's discretion per CONTEXT.md — RESEARCH.md's Assumption A1 confirms low risk either way.

---

### `crates/devflow-core/src/agents/mod.rs` — `AgentAdapter::preflight()` (17c, D-13)

**Analog:** `extra_env()` default method, same file, lines 39-41 — exact precedent named in CONTEXT D-13.

**Full trait + default-impl idiom** (lines 11-45):
```rust
pub trait AgentAdapter {
    fn name(&self) -> &'static str;
    fn exec_command(
        &self,
        phase: u32,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>);

    /// Extra environment variables for the agent process tree. ...
    fn extra_env(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    fn completion_signal_detected(&self, output: &str) -> bool;

    // NEW (D-13): mirror extra_env's empty-default shape exactly.
    // fn preflight(&self, state: &crate::state::State) -> Result<(), PreflightError> {
    //     Ok(())
    // }
}
```
Test-coverage analog for adapter-specific overrides (Codex's non-default `extra_env`, lines 152-158 in same file's `#[cfg(test)] mod tests`) — copy this shape for any adapter that overrides `preflight()`:
```rust
#[test]
fn codex_disables_signing_via_env_others_do_not() {
    let env = adapter_for(AgentKind::Codex).extra_env();
    assert!(env.contains(&("GIT_CONFIG_KEY_0".into(), "commit.gpgsign".into())));
    assert!(adapter_for(AgentKind::Claude).extra_env().is_empty());
}
```

---

### `crates/devflow-cli/src/main.rs` — `advance()` dispatch fix (17a, D-01/D-06)

**Analog:** itself — `advance()`, lines 788-887, specifically the D-01 defect site.

**The exact bug to fix** (lines 854-871):
```rust
let failed = matches!(
    result.status,
    AgentStatus::Failed | AgentStatus::RateLimited
);
if failed {
    return match stage { /* Validate/Ship/other → gate paths */ };
}

// Success (or Unknown — advance with the warning already printed above).
match stage {
    Stage::Define => transition(project_root, &mut state, Stage::Plan),
    Stage::Plan => transition(project_root, &mut state, Stage::Code),
    Stage::Code => transition(project_root, &mut state, Stage::Validate),
    // ...
}
```
Fix shape: replace the `matches!(Failed | RateLimited)` boolean gate with either (a) an exhaustive match over `result.status` feeding the new `outcome_policy::decide_action`, or (b) extend the `matches!` set with every non-Success variant — CONTEXT.md D-02 explicitly prefers the exhaustive-match approach ("forces every future stage/outcome pair to be handled"). Every stage (D-06) gets this — including `Stage::Define`/`Stage::Plan`, which currently have NO gate path in the failure arm at all except the shared `_ => handle_stage_failure(...)` default.

---

### `crates/devflow-cli/src/main.rs` — `run_preflight` (17c, NEW, D-15/D-16)

**Analog:** `handle_stage_failure` (981-1009) for the gate+notify dispatch shape; `ensure_agent_binary`/`agent_binary_available` (657-687) as the existing (sole) preflight precedent; `launch_stage` (692 onward) as the call site.

**Existing preflight precedent to extend, not replace** (lines 673-687):
```rust
fn agent_program(agent: AgentKind) -> &'static str {
    agents::adapter_for(agent).exec_command(0, "", &[]).0
}

fn ensure_agent_binary(program: &str) -> Result<(), CliError> {
    if agent_binary_available(program) {
        return Ok(());
    }
    Err(CliError::Message(format!(
        "agent binary `{program}` not found — is it installed? (run `devflow doctor`)"
    )))
}
```

**Gate+notify dispatch shape to copy** (`handle_stage_failure`, lines 981-1009 — `run_gate` + `GateAction` match is the WR-11 never-silent idiom D-15 must reuse verbatim, NOT a hard exit):
```rust
fn handle_stage_failure(
    project_root: &Path,
    state: &mut State,
    stage: Stage,
    reason: Option<String>,
) -> Result<(), CliError> {
    let context = format!(
        "[never-silent] stage {stage} failed: {} — human review needed (retry, loop-to-code, or abort)",
        truncate_reason(&reason.unwrap_or_else(|| "no details available".into()))
    );
    match run_gate(project_root, state, stage, &context)? {
        GateAction::Advance => { /* retry */ }
        GateAction::LoopBack(_) => { /* retry same stage */ }
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}
```
RESEARCH.md's illustrative `run_preflight` (Code Examples section) composes `generic_preflight_checks(state).and_then(|()| adapter.preflight(state))` then routes failure through this exact `run_gate`/`GateAction` shape — use that as the wiring template, called from inside `launch_stage` per D-16 (before every stage launch, scoped to that stage).

---

### `crates/devflow-cli/build.rs` (NEW, D-20)

**Analog:** none in-tree (first `build.rs` in the workspace). Use RESEARCH.md's cited cargo idiom directly — cross-checked against `cargo:rustc-env`/`cargo:rerun-if-changed` convention:
```rust
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");

    let commit = run_git(&["rev-parse", "HEAD"]);
    let dirty = run_git(&["status", "--porcelain"])
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    // D-20: MUST degrade gracefully when git metadata is unavailable
    // (crates.io installs have no .git) — absence of provenance is not staleness.
    println!("cargo:rustc-env=DEVFLOW_BUILD_COMMIT={}", commit.unwrap_or_default());
    println!("cargo:rustc-env=DEVFLOW_BUILD_DIRTY={dirty}");
    println!(
        "cargo:rustc-env=DEVFLOW_BUILD_TIMESTAMP={}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
}

fn run_git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
```
Follow the codebase's existing git-shelling idiom (`std::process::Command::new("git")`, matching `evaluate_layer2`/`evaluate_layer3`'s git subprocess calls in `agent_result.rs`) rather than a `git2` binding — consistent with D-20's zero-new-dependency constraint. No `chrono`/`time` dependency — hand-roll the Unix-seconds timestamp per `ship.rs`'s existing no-date-crate precedent.

**Runtime staleness check (D-19)** — companion to `build.rs`, lives in `devflow-cli` or `devflow-core`, follows the same git-shell idiom as `evaluate_layer2`/`evaluate_layer3`:
```rust
fn embedded_commit_is_stale(project_root: &Path, embedded_commit: &str) -> Staleness {
    if embedded_commit.is_empty() {
        return Staleness::Unknown; // absence of provenance != staleness
    }
    let output = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", embedded_commit, "HEAD"])
        .current_dir(project_root)
        .output();
    match output.map(|o| o.status.code()) {
        Ok(Some(0)) => Staleness::Fresh,
        Ok(Some(1)) => Staleness::Stale,
        _ => Staleness::Indeterminate, // Pitfall 4: don't hard-block on error/unknown-commit
    }
}
```

---

### `crates/devflow-cli/src/main.rs` — `workflow_started` extension (17d, D-21)

**Analog:** the exact call site being extended, `main.rs:605-614`.

**Current shape to extend** (add version/commit/dirty/build-timestamp/exe-path fields):
```rust
events::emit(
    project_root,
    phase,
    "workflow_started",
    serde_json::json!({
        "agent": state.agent.to_string(),
        "mode": state.mode.to_string(),
        "worktree": state.worktree_path.as_ref().map(|p| p.display().to_string()),
        // D-21 additions: "version", "commit" (env!("DEVFLOW_BUILD_COMMIT")),
        // "dirty" (env!("DEVFLOW_BUILD_DIRTY")), "build_timestamp", "exe_path"
    }),
);
```

**`current_exe()` precedent** (`crates/devflow-core/src/monitor.rs:79`, cited by D-21 as the exe-path-resolution precedent to reuse):
```rust
let binary = std::env::current_exe()
    // ... (monitor.rs uses this to resolve the devflow binary path for the
    // detached monitor's `devflow advance` invocation — same call, new use site)
```

**A second same-crate `events::emit` shape for the payload style** (`prepare_loop_back_to_code`, lines 1178-1186):
```rust
events::emit(
    project_root,
    state.phase,
    "loop_back",
    serde_json::json!({
        "from": gate_stage.to_string(),
        "consecutive_failures": state.consecutive_failures,
    }),
);
```

---

## Shared Patterns

### `events::emit` — schema v1, append-only, envelope-keys-win
**Source:** `crates/devflow-core/src/events.rs:35-65`
**Apply to:** D-10 (structured evidence on every terminal decision), D-21 (`workflow_started` provenance fields)
```rust
pub fn emit(project_root: &Path, phase: u32, event: &str, fields: serde_json::Value) {
    let mut line = serde_json::json!({
        "v": SCHEMA_VERSION,
        "ts": unix_now(),
        "phase": phase,
        "event": event,
    });
    match fields {
        serde_json::Value::Object(map) => {
            let base = line.as_object_mut().expect("line is an object");
            for (key, value) in map {
                // Envelope keys win — a payload must not be able to forge
                // another phase's identity or a different event kind.
                base.entry(key).or_insert(value);
            }
        }
        serde_json::Value::Null => {}
        other => { line["data"] = other; }
    }
    // append-only write via OpenOptions::new().create(true).append(true) ...
}
```
D-10's evidence record (layer decided, outcome, detail) is FIELDS in this `fields` json object — do not invent a parallel store. Do not change the envelope keys (`v`, `ts`, `phase`, `event`) — the "envelope keys win" guarantee at line 47-48 is load-bearing and must not be worked around by a payload field of the same name.

### `run_gate` / `GateAction` — never-silent gate+notify (WR-11 idiom)
**Source:** `crates/devflow-cli/src/main.rs:1233` (`run_gate`) + `GateAction` dispatch pattern used throughout `advance()`'s failure handlers (891-1027)
**Apply to:** D-15 (preflight failure semantics — named gate + notify, not a hard exit)
```rust
match run_gate(project_root, state, stage, &context)? {
    GateAction::Advance => { /* retry / proceed */ }
    GateAction::LoopBack(_) => { /* loop back to Code, or retry same stage */ }
    GateAction::Abort(reason) => abort(project_root, state, &reason),
}
```

### `TRUST_EXTERNAL_VERIFY_ENV` approval mechanism — UNCHANGED, must be preserved exactly
**Source:** `crates/devflow-core/src/verify.rs:10,17-21,99-107`; consumed in `agent_result.rs:633-690`
**Apply to:** D-05's Layer 0 extension — the approval-mismatch detection (commands reread vs. approved, both directions) is the security property that makes Layer 0 trustworthy. Extend the STAGE SCOPE and the SUCCESS BRANCH only; do not touch the approval-comparison logic.
```rust
pub const TRUST_EXTERNAL_VERIFY_ENV: &str = "DEVFLOW_TRUST_EXTERNAL_VERIFY";
pub fn external_verification_approval() -> Option<Vec<String>> {
    let value = std::env::var(TRUST_EXTERNAL_VERIFY_ENV).ok()?;
    let commands = serde_json::from_str::<Vec<String>>(&value).ok()?;
    (!commands.is_empty()).then_some(commands)
}
```

### `MAX_CONSECUTIVE_FAILURES` counter idiom — precedent for D-08's separate infra counter
**Source:** `crates/devflow-cli/src/main.rs` — `mode::MAX_CONSECUTIVE_FAILURES`, `state.mode.should_gate(stage, state.consecutive_failures)`, `state.consecutive_failures += 1` (line 897) — used at `handle_validate_outcome` (891-925)
**Apply to:** D-08 (separate counter for infrastructure outcomes, never touching `consecutive_failures`) — add a distinct `State` field (e.g. `infra_failures: u32`) with its own ceiling constant, mirroring `MAX_CONSECUTIVE_FAILURES`'s naming/placement in `mode.rs`, but NOT reusing the same field or gate-policy method.

### Git subprocess shelling idiom — no `git2`, exclusively `std::process::Command`
**Source:** `agent_result.rs` (`evaluate_layer2` lines 536-541, 545-548; `evaluate_layer3` lines 598-608)
**Apply to:** `build.rs` (D-20) and the D-19 staleness check — every git interaction in this codebase shells to the `git` binary; follow that exclusively, do not introduce `git2`.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/devflow-cli/build.rs` | config (build script) | batch | First `build.rs` anywhere in the workspace — no in-tree precedent. Use RESEARCH.md's cited standard cargo `build.rs` idiom (Code Examples section) verbatim as the pattern source instead of a codebase analog. |

## Metadata

**Analog search scope:** `crates/devflow-core/src/{agent_result.rs, agents/mod.rs, events.rs, verify.rs, config.rs, monitor.rs}`, `crates/devflow-cli/src/main.rs`
**Files scanned:** 7 source files read directly (targeted, non-overlapping ranges); `rg` used to locate all `fn advance|evaluate_layer*|events::emit|current_exe|MAX_CONSECUTIVE_FAILURES` sites before reading
**Pattern extraction date:** 2026-07-18
