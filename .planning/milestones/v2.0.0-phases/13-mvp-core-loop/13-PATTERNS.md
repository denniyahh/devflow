# Phase 13: MVP Core Loop - Pattern Map

**Mapped:** 2026-07-14
**Files analyzed:** 6 (all modified, no new files)
**Analogs found:** 6 / 6 (this phase extends existing functions in-place; every "analog" is an existing sibling function/pattern in the same file)

## File Classification

This phase introduces **no new files** — RESEARCH.md's "Recommended Project Structure" confirms all work is extension/deletion inside existing modules. The table below classifies each *modified* file by role/data-flow and points to the existing in-file (or sibling-file) pattern each change should mirror.

| Modified File | Role | Data Flow | Closest Analog (in-repo) | Match Quality |
|---------------|------|-----------|---------------------------|----------------|
| `crates/devflow-cli/src/main.rs` (`advance()` catch-all arm, 13c/WR-11) | controller (state-machine) | event-driven | `handle_validate_outcome()` (same file, lines 389-423) | exact — same file, same function family |
| `crates/devflow-cli/src/main.rs` (`handle_ship_outcome()`, 13a) | controller (state-machine) | event-driven | `handle_validate_outcome()` (same file, lines 389-423) | exact — explicitly named as the template in RESEARCH.md Pattern 3 |
| `crates/devflow-cli/src/main.rs` (notify hook call site, 13c) | utility (shell-out) | request-response (fire-and-forget) | `hooks.rs::docs_update` (lines 117-137) | exact — fail-soft shell-out idiom |
| `crates/devflow-cli/src/main.rs` (`GATE_TIMEOUT_SECS` → env override, 13c) | config | transform | `main.rs::main()` `DEVFLOW_LOG_FORMAT` read (lines 178-185) | exact — established env-var-config idiom |
| `crates/devflow-cli/src/main.rs` (`Start.worktree` default flip, 13d) | config (CLI arg) | CRUD (flag parsing) | `Start` struct's existing `#[arg(long)]` fields (lines 30-52) | exact — same struct, same clap convention |
| `crates/devflow-core/src/agent_result.rs` (`is_error`/`num_turns` reads, 13b) | service (parser) | transform | `extract_json_result_text` (lines 131-138) / `detect_claude_rate_limit` (lines 81-95) | exact — same file, same envelope-unwrap idiom |
| `crates/devflow-core/src/agent_result.rs` (new Codex JSONL parser, 13b) | service (parser) | batch (line-by-line over captured stdout) | `detect_codex_rate_limit` (lines 97-127) — same "search captured Codex stdout as text" precedent, now needs a JSONL variant | role-match (new sub-pattern, not exact) |
| `crates/devflow-core/src/agent_result.rs` (stage-scoped Layer 2, 13b) | service | CRUD (branch/commit inspection) | `evaluate_layer2` (lines 240-296) | exact — extend existing function's stage parameter |
| `crates/devflow-core/src/ship.rs` (delete dead v1 code, 13a) | service | CRUD (file-based bookkeeping) | N/A — deletion only; surviving code is `prepend_changelog`, `shell_quote`, `CronInstructions`/cron-schedule helpers | n/a (deletion) |

## Pattern Assignments

### `handle_ship_outcome()` failure branch (13a) + `advance()` catch-all fix (13c/WR-11)

**Analog:** `handle_validate_outcome()` — `crates/devflow-cli/src/main.rs` lines 389-423

**Full analog function (this is the shape to copy):**
```rust
// crates/devflow-cli/src/main.rs:389-423
fn handle_validate_outcome(
    project_root: &Path,
    state: &mut State,
    passed: bool,
) -> Result<(), CliError> {
    if !passed {
        state.consecutive_failures += 1;
        workflow::save_state(state)?;
    }

    if state
        .mode
        .should_gate(Stage::Validate, state.consecutive_failures)
    {
        let context = if passed {
            "Validation passed — approve to ship?".to_string()
        } else {
            format!(
                "Validation failed {} time(s) — human review needed.",
                state.consecutive_failures
            )
        };
        return match run_gate(project_root, state, Stage::Validate, &context)? {
            GateAction::Advance => transition(project_root, state, Stage::Ship),
            GateAction::LoopBack(_) => loop_back_to_code(project_root, state),
            GateAction::Abort(reason) => abort(project_root, state, &reason),
        };
    }

    if passed {
        transition(project_root, state, Stage::Ship)
    } else {
        loop_back_to_code(project_root, state)
    }
}
```

**Current (incomplete) Ship handler to extend** — `crates/devflow-cli/src/main.rs` lines 425-437:
```rust
/// Decide what happens after the Ship stage completes — always gated.
fn handle_ship_outcome(project_root: &Path, state: &mut State) -> Result<(), CliError> {
    match run_gate(
        project_root,
        state,
        Stage::Ship,
        "Ship complete — approve merge?",
    )? {
        GateAction::Advance => finish_workflow(project_root, state),
        GateAction::LoopBack(_) => loop_back_to_code(project_root, state),
        GateAction::Abort(reason) => abort(project_root, state, &reason),
    }
}
```
This function is only ever called from the *success* arm of `advance()` (line 383: `Stage::Ship => handle_ship_outcome(...)`) — it is never reached on failure today, because Ship falls into the catch-all `_ => Err(...)` arm below. 13a's job: give `advance()` a `Stage::Ship` failure arm (parallel to the existing `Stage::Validate` arm) that routes into a **new** `ReviewFailed`/`AgentFailed`-aware Ship-failure handler modeled on `handle_validate_outcome`'s gate/loop-back/abort shape, but per Pitfall 3 (see RESEARCH.md), **do not** generalize `handle_validate_outcome` itself — write a small new function (e.g. `handle_stage_failure`) so Validate's `consecutive_failures`/auto-loop semantics don't leak onto Ship/other stages.

**The exact `advance()` block to change (WR-11 / 13c + 13a):**
```rust
// crates/devflow-cli/src/main.rs:359-375
let failed = matches!(
    result.status,
    AgentStatus::Failed | AgentStatus::RateLimited
);
if failed {
    return match stage {
        // Validate failures drive the Code↔Validate loop (or a gate).
        Stage::Validate => handle_validate_outcome(project_root, &mut state, false),
        // Other stages have no auto-loop — halt and leave state for recovery.
        _ => Err(CliError::Message(format!(
            "stage {stage} failed: {}",
            result
                .reason
                .unwrap_or_else(|| "no details available".into())
        ))),
    };
}
```
The `_ => Err(...)` arm is WR-11's silent-halt bug: it returns an error but never calls `run_gate`/never writes a gate file, so `gate_pending` stays `false` and nothing surfaces the halt (no notification). Per RESEARCH.md Pitfall 3, the fix should add a new stage-parametric arm (e.g. `_ => handle_stage_failure(project_root, &mut state, stage, result.reason)`) that always fires a gate + the new notify hook and never auto-loops — structurally separate from `handle_validate_outcome`.

---

### Notify hook (13c)

**Analog:** `hooks.rs::docs_update` — `crates/devflow-core/src/hooks.rs` lines 117-137 (fail-soft shell-out idiom)

```rust
// crates/devflow-core/src/hooks.rs:117-137
fn docs_update(ctx: &HookContext) -> Result<(), HookError> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("cargo doc --no-deps 2>&1")
        .current_dir(&ctx.project_root)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            // Commit any doc changes; ignore "nothing to commit".
            let git = GitFlow::new(&ctx.project_root);
            if let Err(err) = git.commit_all("docs: update generated docs") {
                warn!("DocsUpdate: commit failed: {err}");
            } else {
                info!("DocsUpdate: docs regenerated and committed");
            }
        }
        Ok(_) => warn!("DocsUpdate: cargo doc reported a failure; skipping commit"),
        Err(err) => warn!("DocsUpdate: could not run cargo doc: {err}"),
    }
    Ok(()) // always Ok — fail-soft
}
```
Copy this shape exactly for the new notify hook: read `DEVFLOW_GATE_NOTIFY_CMD` from env, skip silently if unset, `Command::new("sh").arg("-c").arg(cmd)`, `warn!` on non-zero exit or spawn error, always return `Ok(())`. **Security note (V5, ASVS):** per RESEARCH.md's Security Domain section, do NOT string-interpolate gate context (phase/stage/human text) into the `sh -c` command string — pass context via env var or stdin to the user-configured command, following the existing WR-01 precedent (`monitor.rs::shell_escape`, argv-based `Command::new(program).args(&args)` rather than string concatenation).

**Call site to wire into:** `run_gate()` — `crates/devflow-cli/src/main.rs` lines 505-529 (fire the hook immediately after `Gates::write_gate(...)`, before or after the `println!` at line 514-517).

---

### `GATE_TIMEOUT_SECS` env override (13c)

**Analog:** `main()`'s `DEVFLOW_LOG_FORMAT` read — `crates/devflow-cli/src/main.rs` lines 178-185

```rust
// crates/devflow-cli/src/main.rs:178-185
fn main() {
    match std::env::var("DEVFLOW_LOG_FORMAT").as_deref() {
        Ok("json") => {
            tracing_subscriber::fmt().json().init();
        }
        _ => {
            tracing_subscriber::fmt::init();
```
Apply the same one-line-parse-with-fallback shape to the existing constant at `main.rs:16`:
```rust
// crates/devflow-cli/src/main.rs:16 (current)
const GATE_TIMEOUT_SECS: u64 = 7 * 24 * 60 * 60;
```
Replace the const with a function/lazy read: `std::env::var("DEVFLOW_GATE_TIMEOUT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(7 * 24 * 60 * 60)`, called at the `run_gate` site (line 518: `Gates::poll_response(project_root, state.phase, stage, GATE_TIMEOUT_SECS)`).

---

### `Start.worktree` default flip (13d)

**Analog:** the `Start` subcommand's existing `#[arg(long)]` fields — `crates/devflow-cli/src/main.rs` lines 30-52

```rust
// crates/devflow-cli/src/main.rs:30-48 (current)
Start {
    /// Phase number to work on.
    #[arg(long)]
    phase: u32,
    /// Agent to launch.
    #[arg(long, default_value = "claude")]
    agent: AgentKind,
    /// Pipeline mode: `auto` runs to Ship unattended; `supervise` gates at Validate.
    #[arg(long)]
    mode: Mode,
    /// Overwrite the feature branch if it already exists.
    #[arg(long)]
    force: bool,
    /// Run the agent in an isolated git worktree at `.worktrees/phase-NN/`.
    #[arg(long)]
    worktree: bool,
    ...
```
`agent` already demonstrates the `default_value` clap attribute pattern to copy. For 13d, the flag itself should flip semantics: either rename to `no_worktree: bool` (opt-out) or keep `worktree: bool` but change every call site's boolean sense, per RESEARCH.md's note that `parallel()` and `sequentagent` already call `start(..., true, false)` with an explicit bool and don't read the CLI default — confirm those call sites are unaffected by whichever rename/flip is chosen.

---

### Claude envelope `is_error`/`num_turns` reads (13b)

**Analog:** `extract_json_result_text` — `crates/devflow-core/src/agent_result.rs` lines 129-138

```rust
// crates/devflow-core/src/agent_result.rs:129-138
/// If `stdout` is a JSON result envelope, return the decoded `result` text
/// field (with escapes such as `\n` resolved). Returns `None` for plain text.
fn extract_json_result_text(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    value.get("result")?.as_str().map(str::to_string)
}
```
Add a sibling function (or extend this one to also return `is_error`/`num_turns`) using the identical `serde_json::Value::get(...)` idiom already used by `detect_claude_rate_limit` (lines 81-95, below) for pulling sibling top-level fields off the same envelope object:
```rust
// crates/devflow-core/src/agent_result.rs:81-95 — sibling-field extraction idiom to copy
fn detect_claude_rate_limit(stdout: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    let rate_limited = json_has_str(&value, "subtype", "error_rate_limit")
        || json_has_i64(&value, "api_error_status", 429)
        || json_has_i64(&value, "status", 429)
        || json_has_i64(&value, "status_code", 429);
    if !rate_limited {
        return None;
    }
    json_find_key(&value, "retry_after")
        .and_then(json_scalar_to_string)
        .or_else(|| json_find_key(&value, "message").and_then(json_scalar_to_string))
        .or_else(|| json_find_key(&value, "error").and_then(json_scalar_to_string))
        .or_else(|| Some("usage limit".to_string()))
}
```
New read: `value.get("is_error").and_then(Value::as_bool)` and `value.get("num_turns").and_then(Value::as_u64)` as top-level siblings of `result` on the same envelope, per the confirmed schema in RESEARCH.md's Code Examples section.

---

### Codex JSONL event-stream parser (13b, new sub-pattern)

**Analog:** `detect_codex_rate_limit` — `crates/devflow-core/src/agent_result.rs` lines 97-127 (closest existing "parse captured Codex stdout as text" precedent — no line-by-line JSON parser exists yet, so this is a role-match, not an exact match)

```rust
// crates/devflow-core/src/agent_result.rs:97-127
fn detect_codex_rate_limit(stdout: &str) -> Option<String> {
    let lower = stdout.to_ascii_lowercase();
    if let Some(idx) = lower.find("try again at ") {
        let start = idx + "try again at ".len();
        let retry = stdout[start..]
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .trim_end_matches(['.', ',', ';'])
            .trim();
        if !retry.is_empty() {
            return Some(retry.to_string());
        }
    }
    ...
}
```
New parser (per RESEARCH.md's Don't Hand-Roll table): `stdout.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())`, find the **last** `turn.completed`/`turn.failed` event (iterate all lines — Codex stdout is fully captured before `advance()` runs, per `agent.rs::capture_agent_output`, so no streaming/async parser is needed). Treat `turn.failed` as `AgentStatus::Failed` with `reason` from the event's `error.message`.

---

### Stage-scoped Layer 2 (commit-count fallback) (13b)

**Analog:** `evaluate_layer2` — `crates/devflow-core/src/agent_result.rs` lines 240-296

```rust
// crates/devflow-core/src/agent_result.rs:240-296 (signature + core logic, current)
/// Layer 2: Use exit code + commit count to determine result.
///
/// Counts commits in `feature/phase-NN` branch (if it exists).
///   exit=0, commits>0 → advance (probable ok)
///   exit=0, commits=0 → halt "no work done"
pub fn evaluate_layer2(
    ...
) -> AgentResult {
    // Verify branch exists before counting commits.
    ...
    let commits: u32 = if branch_exists {
        ...
    status: if exit_code == 0 && commits > 0 {
        ...
    } else if commits == 0 {
        format!(
            "no commits found on {} (agent exit code was {})",
            ...
        )
    } else {
        ...
    }
}
```
13b needs this function (or its caller in `evaluate_agent_result`) to become stage-aware: skip the "commits == 0 → halt" failure path entirely for `Stage::Define`/`Stage::Validate` (which legitimately produce zero commits), keeping the existing commit-gate logic only for Code-like stages. Add a `stage: Stage` parameter (or branch in the caller before invoking Layer 2) rather than changing the function's core commit-counting logic.

## Shared Patterns

### Fail-soft shell-out (applies to: notify hook, 13c)
**Source:** `crates/devflow-core/src/hooks.rs::docs_update` (lines 117-137)
Always `Command::new("sh").arg("-c").arg(cmd)`, log with `warn!`/`info!` (already imported via `tracing`), never propagate the error upward — return `Ok(())` unconditionally.

### Env-var-driven runtime config, no config file (applies to: notify-hook command, gate timeout, 13c)
**Source:** `crates/devflow-cli/src/main.rs::main()` (lines 178-185, `DEVFLOW_LOG_FORMAT`)
`std::env::var("KEY").ok().and_then(|s| s.parse().ok()).unwrap_or(default)` — this is the *only* pattern-consistent option; a `devflow.toml`/`.devflow.yaml` config file was explicitly eliminated/shelved (2026-06-19, reaffirmed 2026-07-08 per STATE.md) and must not be reopened for this phase.

### Stage-parametric gate/loop-back/abort (applies to: Ship failure branch 13a, WR-11 catch-all fix 13c)
**Source:** `crates/devflow-cli/src/main.rs::handle_validate_outcome` (lines 389-423) and `run_gate` (lines 505-529)
`run_gate(...)` returns a `GateAction` (`Advance` | `LoopBack(Stage)` | `Abort(String)`, defined in `gates.rs` lines 56-79) that every stage-outcome handler should `match` on identically. Do not generalize `handle_validate_outcome` itself to serve Ship too (Pitfall 3) — write a new function with the same three-arm match shape.

### `GateAction::from_response` reason-string convention (relevant to 13a's `ReviewFailed`/`AgentFailed` open question)
**Source:** `crates/devflow-core/src/gates.rs::GateAction::from_response` (lines 65-79)
```rust
pub fn from_response(response: &GateResponse) -> GateAction {
    if response.approved {
        return GateAction::Advance;
    }
    match response.note.as_deref() {
        Some(note) if note.to_ascii_lowercase().contains("abort") => {
            GateAction::Abort(note.to_string())
        }
        _ => GateAction::LoopBack(Stage::Code),
    }
}
```
This is the precedent RESEARCH.md's Open Question #1 points to: a `note.contains("abort")` string-convention rather than a new enum variant. Per RESEARCH.md's recommendation, prefer extending `AgentResult.reason: Option<String>` with a similar string convention (e.g. a `"review:"` prefix) for `ReviewFailed` vs. a new `AgentStatus` variant — lower blast radius, no serde format break on the existing `#[serde(rename_all = "lowercase")]` enum (`agent_result.rs` lines 24-35).

## No Analog Found

None — this phase touches only existing files, and every change has a same-file or same-crate sibling pattern to copy, as confirmed above.

## Metadata

**Analog search scope:** `crates/devflow-cli/src/main.rs`, `crates/devflow-core/src/{agent_result,hooks,gates,ship}.rs` — the exact file set RESEARCH.md's "Recommended Project Structure" and "Architectural Responsibility Map" identify as in-scope; no broader repo search was needed since RESEARCH.md already traced every claim to specific file/line reads.
**Files scanned:** 6 (all read directly this session with line-number citations above)
**Pattern extraction date:** 2026-07-14
