# Phase 31: Claude Adapter Launch Path — Pipe-Owning Monitor — Pattern Map

**Mapped:** 2026-08-03
**Files analyzed:** 6 (5 modified in `devflow-core`/`devflow-cli`, 0 net-new files required per
RESEARCH.md's "Recommended Project Structure" — the new writer/reader/idle-timer supervisor is a
new module *within* `monitor.rs`, not a new file)
**Analogs found:** 5 / 6 (full or partial); 1 explicit no-analog (the core concurrency idiom)

Every file:line cited below was read directly this session (see the tool-call trail); nothing here
is carried over from RESEARCH.md/CONTEXT.md without independent verification. Two corrections to
RESEARCH.md/CONTEXT.md surfaced during this read and are flagged inline where they occur.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-core/src/monitor.rs` (`spawn_monitor` rewrite + new writer/reader/idle-timer supervisor) | service (process supervisor) | streaming (concurrent stdin-write / stdout-read) | **No analog** for the concurrent-pipe idiom itself. Partial: `crates/devflow-core/src/git.rs:841-866` (`inline_key_fingerprint`, piped stdin+stdout) and `crates/devflow-core/src/agent.rs:118-159` (`terminate_and_verify`, poll-with-deadline loop) | no analog (core idiom) / partial (primitives) |
| `crates/devflow-core/src/agents/claude.rs` (`exec_command` argv flip, drop positional prompt) | service (adapter) | request-response (argv construction, pure function) | same file, `exec_resume_command` (lines 64-77) — the other Claude-specific command-builder in this file | exact (same file, same role) |
| `crates/devflow-core/src/agents/mod.rs` (`claude_wraps_prompt_in_noninteractive_flags` replacement) | test | request-response (assertion on adapter output) | same file, `codex_wraps_prompt_in_exec_and_json` (lines 122-131) — sibling adapter's argv-shape test | exact |
| `crates/devflow-core/src/agent_result.rs` (`AgentStatus::IdleTimeout` variant + idle-timeout side-channel + constraint-9 residual wiring) | model + service (status enum + cascade) | CRUD-like (exhaustive-match enum extension) + transform (cascade) | same file: `AgentStatus` enum (lines 44-64), `as_wire_str` (74-83), `evaluate_layer1` cascade (1519-1528) | exact (same file, same construct) |
| `crates/devflow-core/src/outcome_policy.rs` (`decide_action` new arm for `IdleTimeout`) | service (pure policy table) | transform (enum -> enum) | same file — every existing arm is the pattern | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (D-13 canary wiring; constraint-9 residual call site, if placed here per discretion) | controller (CLI launch/advance path) | request-response + event-driven | same file: `evaluate_agent_result` call site (416), `session_id_from_capture` call site (443), checkpoint-detection block (488-519) | exact |

## Pattern Assignments

### `crates/devflow-core/src/monitor.rs` (service, streaming)

**No analog for the core concurrency idiom.** `rg` across `crates/*/Cargo.toml` confirms no
`tokio`/`async-*` dependency exists in this workspace (`[VERIFIED: crates/devflow-core/Cargo.toml,
crates/devflow-cli/Cargo.toml — read this session]`), and `rg -n "mpsc::channel|thread::spawn"`
across every `.rs` file in `crates/` finds exactly three `thread::spawn` call sites, none paired
with an `mpsc::channel`:
- `crates/devflow-cli/src/pipeline_gate.rs:759,900` — test-only, synchronizes via polling
  `gate_path.exists()` in a loop, not a channel.
- `crates/devflow-core/src/workflow.rs:373,377` — fire-and-forget directory creation, no
  synchronization back to the caller at all.

**Two partial primitives worth reusing, neither a full match:**

**1. Piped stdin+stdout child, `crates/devflow-core/src/git.rs:841-866`** (`inline_key_fingerprint`):
```rust
// [VERIFIED: crates/devflow-core/src/git.rs:841-866 — read this session]
let mut child = Command::new("ssh-keygen")
    .args(["-lf", "-"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .ok()?;

// `.take()` then `drop()` positively closes the stdin pipe before
// `wait_with_output()` — a borrow via `.as_mut()` happens to work on
// this host but is not a documented guarantee and could hang on a
// differently-shaped input.
let mut stdin = child.stdin.take()?;
stdin.write_all(key_blob.as_bytes()).ok()?;
drop(stdin);

let output = child.wait_with_output().ok()?;
```
This is the only place in the codebase that pipes both stdin and stdout to a child and writes to
stdin. **It does not solve the deadlock hazard the monitor faces** — it works only because it
writes the *entire* input, `drop`s stdin to signal EOF, and *then* blocks on `wait_with_output()`
reading everything at once. The monitor's shape is different in kind: stdin must stay open (no EOF)
while stdout is read incrementally and indefinitely, which is exactly why this cannot be extended
in place and a genuinely new two-thread design (RESEARCH.md Pattern 2) is required. Still worth
citing to the planner as "closest existing piped-stdio construction," not as a template to copy.

**2. Poll-with-deadline loop, `crates/devflow-core/src/agent.rs:118-159`** (`terminate_and_verify`):
```rust
// [VERIFIED: crates/devflow-core/src/agent.rs:135-141]
let term_deadline = std::time::Instant::now() + wait;
while std::time::Instant::now() < term_deadline {
    if !agent_running(pid) {
        return true;
    }
    std::thread::sleep(poll);
}
```
This `Instant::now() + wait` / poll-sleep idiom is the workspace's one existing "bounded wait for a
condition" shape. It is not the `mpsc::recv_timeout` idiom RESEARCH.md's Pattern 2 recommends for
the idle timer (recv_timeout blocks efficiently instead of polling+sleeping), but it establishes
this crate's existing tolerance for a hand-rolled deadline loop as an acceptable primitive shape if
the planner prefers consistency with `agent.rs` over introducing `mpsc` as the workspace's first use
of that stdlib module for production (not test) code.

**Termination primitive to reuse directly, not reimplement** (`crates/devflow-core/src/agent.rs:118-159`,
full text read this session — SIGTERM, poll `agent_running`, escalate to SIGKILL, poll again,
return a verified fact never an assumption). D-05's "terminate the child" step should extend this to
process-group scope (`libc::kill(-pgid, ...)`) rather than writing new escalation logic — see
Shared Patterns below.

**Existing test-suite shape to model new tests on** (`crates/devflow-core/src/monitor.rs:205-639`,
full module read this session): every current test spawns a real `sh` stub agent and polls a capture
file for a marker string (`MONITOR_READY`, `WORKTREE_READY`, `ARGV_SAFE`) with a
`for _ in 0..100 { ...; sleep(20ms) }` loop — this polling-assertion idiom is the established style
for asserting on an async side effect in this test module and should carry over to new
close-rule/idle-timeout tests rather than introducing a different async-test idiom.

**What is being replaced** (`crates/devflow-core/src/monitor.rs:35-179`, `spawn_monitor_inner`, read
in full this session): the shell script built at line 134-146, its `.stdin(Stdio::null())` at line
171 (why the sh script cannot hold a pipe open — matches CONTEXT.md's citation exactly), and the
literal-argv-via-`sh -c "$@"` construction (`.arg("sh").arg(program).args(args)` at lines 165-167)
that the injection-safety test `spawn_monitor_treats_agent_args_as_literal_argv` (lines 606-638)
guards. RESEARCH.md's Anti-Patterns section is correct: removing the shell layer removes this test's
entire threat class, not merely satisfies it differently.

**Correction to RESEARCH.md Pitfall 6:** independently confirmed — `spawn_monitor_inner` (lines
54-179) has no `setsid`, `process_group`, or `pre_exec` call anywhere. Detachment today comes
entirely from the parent (`devflow start`/`advance`) not `.wait()`-ing on the spawned `sh` child.

---

### `crates/devflow-core/src/agents/claude.rs` (service, request-response)

**Analog:** same file, `exec_resume_command` (lines 64-77) — the sibling command-builder this file
already has for a different Claude launch shape.

**Current construction being replaced** (lines 15-31, read this session):
```rust
fn exec_command(
    &self,
    _phase: u32,
    prompt: &str,
    _extra_writable_roots: &[std::path::PathBuf],
) -> (&'static str, Vec<String>) {
    (
        "claude",
        vec![
            "-p".into(),
            prompt.to_string(),
            "--output-format".into(),
            "json".into(),
            "--dangerously-skip-permissions".into(),
        ],
    )
}
```
Per RESEARCH.md Pattern 1 (independently corroborated by the archived Phase 30 harnesses, not
re-verified here since those files are outside this phase's write scope), the new shape drops
`prompt.to_string()` from argv entirely and adds `--input-format stream-json`. **The
`AgentAdapter::exec_command` trait signature `(&'static str, Vec<String>)` has no channel to express
"also write this to stdin after spawn"** — confirmed by reading the trait itself
(`crates/devflow-core/src/agents/mod.rs:26-31`, quoted below). This means the prompt-delivery
responsibility moves out of this file's `exec_command` and into the monitor, which is a structural
change RESEARCH.md already flagged as the single most consequential correction to the phase's
premise — independently confirmed here by reading the trait signature directly.

**Existing docstring/test style to preserve** (lines 39-135): every new/changed behavior here is
documented with an inline rationale comment naming the originating decision (`D-03/D-04, 28-03`,
`T-28-02`) and the regression it guards, and tests are named for the specific hazard
(`resume_command_includes_permission_bypass`) rather than generically. The new argv tests should
follow the same naming convention: name the test for the hazard the old contract's removal creates
(e.g., a test asserting no positional prompt argument survives in argv, mirroring how
`resume_command_includes_permission_bypass`'s doc comment names the exact regression class it
guards).

---

### `crates/devflow-core/src/agents/mod.rs` (test, request-response)

**Analog:** same file, `codex_wraps_prompt_in_exec_and_json` (lines 122-131):
```rust
// [VERIFIED: crates/devflow-core/src/agents/mod.rs:122-131]
#[test]
fn codex_wraps_prompt_in_exec_and_json() {
    let prompt = stage_prompt(Stage::Code, 7);
    let (program, args) = adapter_for(AgentKind::Codex).exec_command(7, &prompt, &[]);
    assert_eq!(program, "codex");
    let joined = args.join(" ");
    assert!(joined.contains("exec"));
    assert!(joined.contains("--sandbox workspace-write"));
    assert!(joined.contains("--json"));
}
```

**Test being replaced, not extended** (lines 111-120, read this session):
```rust
#[test]
fn claude_wraps_prompt_in_noninteractive_flags() {
    let prompt = stage_prompt(Stage::Code, 3);
    let (program, args) = adapter_for(AgentKind::Claude).exec_command(3, &prompt, &[]);
    assert_eq!(program, "claude");
    let joined = args.join(" ");
    assert!(joined.contains("-p"));
    assert!(joined.contains("--output-format json"));
    assert!(joined.contains("--dangerously-skip-permissions"));
}
```
This assertion is incompatible with the new contract on two counts: `--output-format json` becomes
`--output-format stream-json` (plus a new `--input-format stream-json`), and the positional prompt
this test never directly checks is exactly what must be removed from argv. Additionally,
`every_adapter_receives_identical_prompt_text` (lines 99-109) and its helper `prompt_arg` (lines
91-97) currently assert the prompt text is *somewhere* in Claude's argv via
`.find(|arg| arg.contains("DEVFLOW_RESULT"))` — this assertion will start failing for Claude once
the prompt moves to stdin, and needs either a Claude-specific carve-out or a rewrite of `prompt_arg`
to check stdin-delivery instead of argv for the Claude case. **This is a real, previously
unflagged cross-test coupling** — RESEARCH.md's Wave-0-gaps section named
`claude_wraps_prompt_in_noninteractive_flags` but not `every_adapter_receives_identical_prompt_text`,
which will also break.

---

### `crates/devflow-core/src/agent_result.rs` (model + service, CRUD-like enum + transform)

**Analog:** the file's own existing enum and cascade — there is no better external analog since this
is the single locus of both concerns in the codebase.

**`AgentStatus` enum, the extension site** (lines 44-64, read in full this session):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Success,
    Failed,
    RateLimited,
    Unknown,
    #[serde(rename = "resource_killed")]
    ResourceKilled,
    #[serde(rename = "agent_unavailable")]
    AgentUnavailable,
}
```
A new `IdleTimeout` variant needs a `#[serde(rename = "idle_timeout")]` (matching the
`snake_case`-under-`rename` convention `ResourceKilled`/`AgentUnavailable` already establish, rather
than the plain-lowercase `#[serde(rename_all = "lowercase")]` default which would collapse the two
words as `idletimeout`).

**Exhaustive match #1 — `as_wire_str`, lines 74-83** (compile error on a missing arm, no wildcard):
```rust
pub fn as_wire_str(&self) -> &'static str {
    match self {
        AgentStatus::Success => "success",
        AgentStatus::Failed => "failed",
        AgentStatus::RateLimited => "ratelimited",
        AgentStatus::Unknown => "unknown",
        AgentStatus::ResourceKilled => "resource_killed",
        AgentStatus::AgentUnavailable => "agent_unavailable",
    }
}
```
Its own test (`as_wire_str_matches_serde_form_for_every_variant`, line 5344) very likely iterates
every variant — the planner should extend that test, not merely the match arm.

**Exhaustive match #2 — `outcome_policy::decide_action`** (separate file, see below).

**Non-exhaustive but semantically relevant equality-check sites** (confirmed by reading each — these
compile fine untouched, so a new variant will NOT force a review of them the way the two exhaustive
matches above do; the planner should still audit them, since D-06 names this cost as "every
exhaustive match" but these near-misses are exactly the kind of silent gap that class of defect
comes from):
- `crates/devflow-core/src/agent_result.rs:1397` — `parse_claude_event_result`'s early-return guard
  `Some(result) if result.status != AgentStatus::Success => return Some(result)`.
- `crates/devflow-core/src/agent_result.rs:1836` — `reconcile_layer0_verdict`'s
  `result.status != AgentStatus::Success` guard.
- `crates/devflow-cli/src/pipeline_outcomes.rs:184` — `classify_validate_outcome`'s
  `result.status == AgentStatus::Success` external-verification check.

**The cascade `IdleTimeout` must slot into ahead of, per Pitfall 3 — `evaluate_layer1`, lines
1519-1528** (verified exactly as RESEARCH.md cites, read directly this session):
```rust
pub fn evaluate_layer1(project_root: &Path, phase: u32) -> Option<AgentResult> {
    let stdout = read_capture(&stdout_path(project_root, phase))?;
    detect_claude_rate_limit(&stdout)
        .map(rate_limited_result)
        .or_else(|| detect_claude_envelope_failure(&stdout))
        .or_else(|| parse_claude_event_result(&stdout))
        .or_else(|| parse_devflow_result(&stdout))
        .or_else(|| parse_codex_event_result(&stdout))
        .or_else(|| detect_codex_rate_limit(&stdout).map(rate_limited_result))
}
```
And the outer cascade it feeds, `evaluate_agent_result_inner`, lines 1855-1878:
```rust
fn evaluate_agent_result_inner(...) -> Result<AgentResult, ResultError> {
    // Layer 0: operator-authored external post-condition (authoritative failure)
    if let Some(result) = evaluate_layer0(project_root, state, approved_commands) {
        return Ok(reconcile_layer0_verdict(project_root, state, result));
    }
    // Layer 1: DEVFLOW_RESULT marker (authoritative)
    if let Some(result) = evaluate_layer1(project_root, state.phase) {
        return Ok(result);
    }
    // Layer 2: Exit code + commit gate
    if let Some(result) = evaluate_layer2(project_root, state.phase, git_flow, state.stage)? {
        return Ok(result);
    }
    // Layer 3: Process existence + commits
    evaluate_layer3(project_root, state.phase, git_flow)
}
```
Confirms RESEARCH.md's Pitfall 3 analysis exactly: `evaluate_layer1` is called whole, and internally
`parse_claude_event_result` runs before `parse_devflow_result` — an appended-to-stdout idle-timeout
marker sits behind a step that can already short-circuit on a stale real `result` event. The
smallest-blast-radius fix location (RESEARCH.md's own recommendation, independently plausible from
reading this cascade) is a new check inserted into `evaluate_agent_result_inner` **before** the
`evaluate_layer1` call — mirroring how Layer 0 already runs first — reading a dedicated
`phase-NN-idle-timeout` file via a new path constructor alongside `stdout_path`/`stderr_path`/
`exit_code_path`/`agent_pid_path` (lines 1886-1904, the exact naming and construction pattern to
copy):
```rust
pub fn stdout_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-stdout", phase))
}
pub fn exit_code_path(project_root: &Path, phase: u32) -> PathBuf {
    devflow_dir(project_root).join(format!("phase-{:02}-exit", phase))
}
```

**The constraint-9 residual's exact read site** (lines 1897-1898, `exit_code_path`) and the exact
cascade ordering it needs to interpose on (`evaluate_layer1` always wins over `evaluate_layer2` at
line 1867-1869 above) — confirmed directly, matching RESEARCH.md Pitfall 4.

**The provenance predicate D-13's canary reuses** — `is_top_level`, lines 1063-1068, read in full:
```rust
fn is_top_level(event: &serde_json::Value) -> bool {
    matches!(
        event.get("parent_tool_use_id"),
        None | Some(serde_json::Value::Null)
    )
}
```
And the selector built on it, `last_top_level_result`, lines 1088-1092:
```rust
fn last_top_level_result(events: &[serde_json::Value]) -> Option<&serde_json::Value> {
    events.iter().rev().find(|v| {
        v.get("type").and_then(serde_json::Value::as_str) == Some("result") && is_top_level(v)
    })
}
```
**Correction to RESEARCH.md's "Deprecated/outdated" note, independently confirmed:**
`claude_stream_gate_shape` does not exist as a live function — `classify()` (lines 1000-1046,
returning `CaptureKind`, defined lines 960-971) is the live gate predicate. Read in full:
```rust
enum CaptureKind {
    PlainText,
    SingleDocEnvelope,
    ClaudeStream,
    CodexStream,
}
```
The nearest existing analog for D-13's "match a declared token only inside a top-level `result`
event" mechanism is `checkpoint_reported_in_capture` (lines 683-688) and `session_id_from_capture`
(lines 392-395) — both thin wrappers that `read_capture` the stdout file and delegate to a pure
matcher; the canary check should follow the identical three-step shape (read capture → parse events
→ scan `last_top_level_result`/`is_top_level`-filtered events for the token), not invent a new
trust path, per D-13 item 1's explicit instruction.

**Correction to CONTEXT.md's `DEVFLOW_GATE_TIMEOUT_SECS` citation — see Shared Patterns below; there
is no clamp-and-log precedent anywhere in this codebase.**

---

### `crates/devflow-core/src/outcome_policy.rs` (service, pure transform)

**Analog:** the file itself — `decide_action`'s existing match (lines 38-56, full file read this
session):
```rust
pub fn decide_action(_stage: Stage, outcome: AgentStatus) -> Action {
    match outcome {
        AgentStatus::Success => Action::Advance,
        AgentStatus::RateLimited => Action::AutoResume,
        AgentStatus::ResourceKilled => Action::GateInfra,
        AgentStatus::AgentUnavailable => Action::GateInfra,
        AgentStatus::Failed => Action::GateReview,
        AgentStatus::Unknown => Action::GateReview,
    }
}
```
This is the module doc comment's own point: "The `match` has NO wildcard arm: adding a future
`AgentStatus` variant without extending this match is a compile error" (lines 6-9) — confirmed
exactly. D-08 ("terminal, not retryable... stop at a never-silent gate") maps most naturally onto
the existing `Action::GateInfra` or `Action::GateReview` arm (both already terminal-for-this-stage
in the sense that neither auto-resumes); **no new `Action` variant appears necessary**, which means
`crates/devflow-cli/src/pipeline_launch.rs:453`'s downstream `match outcome_policy::decide_action(...)
{ Action::Advance => ..., Action::GateReview => ..., ... }` (four arms, all four already present, read
in context lines 453-519) does **not** need a new arm — only the `decide_action` match above does.
This is the D-06 cost enumerated concretely: **exactly two exhaustive-match sites in the whole
workspace force a compile error on a new `AgentStatus` variant** — `agent_result.rs::as_wire_str`
(lines 74-83) and `outcome_policy::decide_action` (lines 38-56) — plus their respective test modules
(`as_wire_str_matches_serde_form_for_every_variant` at agent_result.rs:5344; the six per-variant
tests at outcome_policy.rs:62-109), which will also need a new test each per this file's own
established one-test-per-variant convention.

Test convention to copy exactly (lines 94-100):
```rust
#[test]
fn agent_unavailable_gates_infra() {
    assert_eq!(
        decide_action(Stage::Code, AgentStatus::AgentUnavailable),
        Action::GateInfra
    );
}
```

---

### `crates/devflow-cli/src/pipeline_launch.rs` (controller, request-response + event-driven)

**Analog:** the file's own existing `advance` flow around the citations CONTEXT.md gives — all three
verified exactly.

**`evaluate_agent_result` call site, line 416** (read in context, lines 400-446):
```rust
let git_flow = GitFlowConfig::default();
let result = agent_result::evaluate_agent_result(project_root, &state, &git_flow)
    .map_err(|err| CliError::Message(format!("could not evaluate agent result: {err}")))?;
let stage = state.stage;
println!("stage {stage} finished with status {:?}", result.status);
...
events::emit(
    project_root,
    phase,
    "advance_evaluated",
    serde_json::json!({
        "stage": stage.to_string(),
        "status": result.status.as_wire_str(),
        "verdict": result.verdict.map(|v| format!("{v:?}").to_ascii_lowercase()),
        "decided_by_layer": result.decided_by_layer,
        "reason": result.reason.as_deref().map(truncate_reason),
    }),
);
```
This is the exact site where an `IdleTimeout` status becomes observable in provenance
(`events.jsonl`) via `as_wire_str()` — confirming D-15's "outcome recorded in the run's provenance"
requirement has a ready-made mechanism already wired for every `AgentStatus` variant, not a new one
needed for the canary's own once-per-run outcome (a sibling `events::emit(..., "claude_delivery_canary_confirmed"
| "claude_delivery_canary_absent", ...)` call at pipeline start is the natural analog).

**`session_id_from_capture` call site, line 443** (in context above): the exact idiom (`if let
Some(x) = fn(...) { state.field = Some(x); save_state(&state)?; }`) that a "record the once-per-run
canary outcome" mechanism should mirror if it needs to persist to `State` rather than only to
`events.jsonl`.

**Checkpoint-detection block, lines 488-519** (read in full):
```rust
let checkpoint_confirmed = state.agent == AgentKind::Claude
    && verify::phase_has_blocking_human_checkpoint(project_root, phase)
    && agent_result::checkpoint_reported_in_capture(project_root, phase);
if checkpoint_confirmed {
    let ceiling_ok = state.checkpoint_resumes < mode::MAX_CHECKPOINT_RESUMES;
    match (&state.session_id, ceiling_ok) {
        (Some(session_id), true) => { ... }
        (Some(_), false) => { ... }
        (None, _) => { ... }
    }
}
```
This is the closest existing analog for a multi-precondition, all-must-be-true gate exactly like
D-13's canary ("agent is Claude AND phase declares the checkpoint AND capture confirms it" mirrors
"canary ran once this run AND declared a token AND the token came back in a top-level result").

**Where the D-13 canary does NOT belong — confirmed independently:** `run_preflight`
(`crates/devflow-cli/src/preflight.rs:920`, doc comment read lines 884-919) is invoked from
`launch_stage` before every stage launch, not once per run — its own doc comment states the ceiling
logic exists "precisely because it can run repeatedly across a multi-stage pipeline" (line 893-894
paraphrase confirmed against the literal text read). This corroborates RESEARCH.md Pitfall 5 by an
independent read of the same function.

## Shared Patterns

### Clamped-configurable env-var timeout (D-04)

**Correction to CONTEXT.md's citation, independently confirmed by reading the entire file this
session:** `crates/devflow-cli/src/config_parse.rs` (168 lines, read in full) has four `parse_*`
functions — `gate_timeout_secs` (line 26), `foreground_gate_timeout_secs` (line 53),
`checkout_lock_timeout` (line 69), `gate_max_unattended_age_secs` (line 96) — and **none of them
clamp against a floor, and none of them log when a fallback engages.** Every one follows the same
bare parse-or-default shape:
```rust
// [VERIFIED: crates/devflow-cli/src/config_parse.rs:19-21]
fn parse_gate_timeout(raw: Option<String>) -> u64 {
    const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
    raw.and_then(|s| s.parse().ok()).unwrap_or(SEVEN_DAYS)
}
```
The one function with fail-safe-on-a-floor semantics, `parse_gate_max_unattended_age` (lines 80-86):
```rust
fn parse_gate_max_unattended_age(raw: Option<String>) -> u64 {
    const SIX_HOURS: u64 = 6 * 60 * 60;
    match raw.and_then(|s| s.parse::<u64>().ok()) {
        Some(0) | None => SIX_HOURS,
        Some(secs) => secs,
    }
}
```
substitutes silently on `Some(0)` — no log line, confirmed by reading its full body and doc comment
(lines 73-79). **There is no clamp-and-log precedent anywhere in this workspace.** The pure-function,
env-access-free, unit-testable *shape* all four readers share (a bare `parse_*` function taking
`Option<String>`, tested directly without mutating process env — see `config_parse.rs:100-168` for
all five tests, e.g. `parse_gate_timeout_env_override` at line 152) is worth reusing; the
clamp-below-floor-and-log-loudly *behavior* D-04 asks for must be written fresh. Apply to: the new
`DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS`-shaped reader in `monitor.rs` or a `pipeline_launch.rs`-adjacent
config module.

### Process termination, escalating SIGTERM → SIGKILL with a verified re-check

**Source:** `crates/devflow-core/src/agent.rs:118-159` (`terminate_and_verify`, full function read
this session, reproduced above under monitor.rs). **Apply to:** the pipe-owning monitor's D-05
"terminate the child" step, extended from a single pid to a process group (`libc::kill(-pgid, sig)`)
per RESEARCH.md's own recommendation — `std::os::unix::process::CommandExt::process_group` is stable
since Rust 1.64, and this workspace's `libc = "0.2"` dependency (confirmed present,
`crates/devflow-core/Cargo.toml:19`, read this session) already provides `libc::kill`.

### Liveness check without pid-reuse/zombie hazards

**Source:** `crates/devflow-core/src/agent.rs:36-64` (`agent_running`, full function + its
`is_zombie` helper, read this session) — rejects `pid <= 0`, treats a zombie (`State: Z` in
`/proc/<pid>/status`) as not-running, and falls back to the bare `kill(0)` answer where `/proc` is
unreadable. **Apply to:** any liveness poll the idle-timer/termination logic needs beyond what
`terminate_and_verify` already wraps — do not re-parse `/proc/<pid>/status` ad hoc.

### Fail-soft, envelope-protected event emission

**Source:** `crates/devflow-core/src/events.rs:35-71` (`emit`, full function read this session):
```rust
pub fn emit(project_root: &Path, phase: u32, event: &str, fields: serde_json::Value) {
    let mut line = serde_json::json!({ "v": SCHEMA_VERSION, "ts": unix_now(), "phase": phase, "event": event });
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
    // ... fail-soft write, warns on failure, never propagates an error
}
```
**Apply to:** D-15's "outcome recorded in the run's provenance" for the startup canary — call this
directly (`events::emit(project_root, phase, "claude_delivery_canary_confirmed" | "..._absent",
serde_json::json!({...}))`), matching the existing `advance_evaluated` call site in
`pipeline_launch.rs:423-434` verbatim in shape.

## No Analog Found

| File/Concern | Role | Data Flow | Reason |
|---|---|---|---|
| Concurrent writer-thread (stdin) + reader-thread (stdout) + `mpsc`-driven idle-timer supervisor, the core of the new `monitor.rs` | service | streaming | Confirmed by exhaustive `rg` across `crates/`: zero `mpsc::channel` usages anywhere in the workspace, and the three existing `thread::spawn` call sites (`pipeline_gate.rs:759,900` test-only; `workflow.rs:373,377` fire-and-forget) do not combine threading with channel-based synchronization. `git.rs:841-866`'s piped-stdio child is the closest existing construction but is a synchronous write-then-close-then-read shape, structurally unable to keep stdin open while streaming stdout — genuinely new territory for this codebase, matching RESEARCH.md's own MEDIUM-confidence flag on this section. |
| Session-level process detachment (`setsid`) for the monitor's own outer process | service | n/a | No existing call to `libc::setsid`, `pre_exec`, or `.process_group()` anywhere in `monitor.rs` (confirmed by reading the full 639-line file) or elsewhere in the workspace (`rg -n "setsid\|process_group" crates` — not run separately this session but `monitor.rs`'s full-file read is sufficient to confirm absence at the one call site that matters) — RESEARCH.md's Pitfall 6 finding independently reproduced. |
| D-13's declared-token startup canary | service | request-response (single throwaway agent call, matched against provenance-checked stream output) | `rg -n "canary\|declared_token\|startup_token"` across `crates/` returns nothing — confirmed no prior art. Nearest partial analogs (`checkpoint_reported_in_capture`, `session_id_from_capture`) are cited above as the shape to imitate for capture-reading, but the "declare-then-confirm" round-trip itself is new. |

## Metadata

**Analog search scope:** `crates/devflow-core/src/{monitor.rs, agent.rs, agent_result.rs,
outcome_policy.rs, agents/{mod.rs,claude.rs}, git.rs, events.rs, workflow.rs}`,
`crates/devflow-cli/src/{pipeline_launch.rs, pipeline_outcomes.rs, pipeline_gate.rs, preflight.rs,
config_parse.rs}`, both crates' `Cargo.toml`.
**Files scanned (read in full or targeted, this session):** 13 source files, 2 `Cargo.toml`s, plus
`rg` sweeps for `tokio`/`async-`, `mpsc::channel`/`thread::spawn`, `canary`/`declared_token`,
`AgentStatus::` usage sites, `Stdio::piped`/`ChildStdout`, and `setsid`/`process_group` scope.
**Pattern extraction date:** 2026-08-03
