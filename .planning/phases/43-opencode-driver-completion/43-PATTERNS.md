# Phase 43: OpenCode Driver Completion - Pattern Map

**Mapped:** 2026-08-23
**Files analyzed:** 4 (2 required edits, 1 required edit to shared tests, 1 optional drive-by)
**Analogs found:** 4 / 4

RESEARCH.md already contains fully-worked, source-grounded code skeletons for every new
function (Patterns 1-4, § Architecture Patterns) modeled line-by-line on the analogs below. This
PATTERNS.md re-derives the same analog selections directly from the current source (not from
RESEARCH.md's copy) and adds the exact current line numbers / excerpts a planner can cite,
including the two shared-test call sites RESEARCH.md's Pitfall 3/4 warns about.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|--------------------|------|-----------|-----------------|----------------|
| `crates/devflow-core/src/agents/opencode.rs` (rewrite the 28-line stub) | driver (AgentDriver impl) | request-response (CLI subprocess spawn) | `crates/devflow-core/src/agents/codex.rs` (argv/delegation shape) + `crates/devflow-core/src/agents/pi.rs` (health/capabilities/test-harness shape) | exact (two-analog composite — no single existing driver combines both concerns) |
| `crates/devflow-core/src/agent_result.rs` — new `is_opencode_event_stream` / `parse_opencode_event_result`, add one `.or_else` line to `evaluate_layer1` | service (stream/event parser) | streaming (JSONL event-stream parse) | `is_codex_event_stream` / `parse_codex_event_result` in the same file (lines 728-849) | exact — explicitly named in CONTEXT.md as the model to copy |
| `crates/devflow-core/src/agents/mod.rs` — update `drivers_reproduce_legacy_adapter_behavior`, `opencode_wraps_prompt_in_run`; remove OpenCode assertion from `default_preflight_is_ok_for_built_in_adapters` | test | request-response (argv assertions) | same file, existing test bodies (lines 219-242, 576-582, 630-641) | exact — these are the files being edited, not analogs |
| `crates/devflow-cli/src/commands.rs` — `doctor_checks()` opencode entry, optional D-11 drive-by | config | request-response | same file, adjacent `cmd_check` entries (lines 2304-2328) | exact |

## Pattern Assignments

### `crates/devflow-core/src/agents/opencode.rs` (driver, request-response)

**Primary analog:** `crates/devflow-core/src/agents/codex.rs` (argv construction + `parse_completion`
delegation). **Secondary analog:** `crates/devflow-core/src/agents/pi.rs` (`health` credential-check
shape, `capabilities` fail-closed subagent probe, `PathGuard`/stub-binary test harness).
**Tertiary analog for the fail-closed *inner-fn* test-isolation split:** `crates/devflow-core/src/agents/hermes.rs`.

**Imports pattern** — current stub (`opencode.rs:1-9`), to extend, not replace:
```rust
//! OpenCode agent driver.
//!
//! Launches `opencode run "<prompt>"` in non-interactive mode.

use crate::phase_id::PhaseId;

/// The modular driver for OpenCode (37-02): positional `opencode run <prompt>`
/// + legacy prompt rendering.
pub struct OpenCodeDriver;

impl super::AgentDriver for OpenCodeDriver {
```
Codex's import block (`codex.rs:1-10`) shows the fuller shape once `health`/`capabilities` are
added — driver files import `super::{AgentDriver, InteractivityMode}` only when they need the
enum; OpenCode needs `super::AgentDriver` (already present) plus `std::path::PathBuf` for the
`_extra_writable_roots` parameter type, exactly as `pi.rs:22-24` does:
```rust
use super::AgentDriver;
use crate::phase_id::PhaseId;
use std::path::PathBuf;
```

**Core `build_command` pattern (D-01)** — model the argv-list shape after Pi's multi-token,
all-separate-elements convention (`pi.rs:54-64`), not a single joined string:
```rust
fn build_command(
    &self,
    _phase: PhaseId,
    prompt: &str,
    _extra_writable_roots: &[PathBuf],
) -> (&'static str, Vec<String>) {
    (
        "pi",
        vec!["-p".into(), "--no-approve".into(), prompt.to_string()],
    )
}
```
Current OpenCode stub to replace (`opencode.rs:20-27`):
```rust
fn build_command(
    &self,
    _phase: PhaseId,
    prompt: &str,
    _extra_writable_roots: &[std::path::PathBuf],
) -> (&'static str, Vec<String>) {
    ("opencode", vec!["run".into(), prompt.to_string()])
}
```
Target shape per D-01/RESEARCH Open Question 1 (two separate argv elements for `--format json`,
matching Codex's `"--sandbox".into(), "workspace-write".into()` convention at `codex.rs:38-39`):
`vec!["run".into(), prompt.to_string(), "--auto".into(), "--format".into(), "json".into()]`.

**`parse_completion` delegation pattern** — copy verbatim shape from Codex (`codex.rs:70-76`):
```rust
/// Relocate the Codex JSONL completion parsing under driver ownership: the
/// function body lives in `agent_result.rs` (where the result-evaluation
/// path and its fixtures live), and this method is the driver's contract
/// entry point for it.
fn parse_completion(&self, output: &str) -> Option<crate::agent_result::AgentResult> {
    crate::agent_result::parse_codex_event_result(output)
}
```
OpenCode's equivalent (per RESEARCH Pattern 2) is a one-line change of the delegate target:
`crate::agent_result::parse_opencode_event_result(output)`.

**`health` credential-check pattern (D-07/D-08)** — copy the shell-out + classify-separately
split from Pi (`pi.rs:66-98` for the method, `pi.rs:104-117` for the pure classifier extracted so
it's unit-testable without spawning a process):
```rust
fn health(&self, _state: &crate::state::State) -> Result<(), String> {
    let provider = configured_pi_provider().unwrap_or_else(|| "google".to_string());
    let output = std::process::Command::new("pi")
        .args([
            "auth",
            "check",
            "--json",
            "--provider",
            &provider,
            "--no-refresh",
        ])
        .output()
        .map_err(|e| format!("could not run `pi auth check`: {e}"))?;
    classify_auth_check(&String::from_utf8_lossy(&output.stdout), output.status.success())
        .map_err(|reason| {
            format!(
                "{reason} for provider `{provider}` — `pi auth check --json --provider {provider}` reports it not ready"
            )
        })
}
```
```rust
/// Map `pi auth check --json` output to a readiness verdict. Split out so the
/// classification is unit-testable without spawning a process.
fn classify_auth_check(stdout: &str, success: bool) -> Result<(), String> {
    let ready = success
        && serde_json::from_str::<serde_json::Value>(stdout)
            .ok()
            .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(str::to_owned))
            .is_some_and(|s| s == "ready");
    if ready {
        Ok(())
    } else {
        Err("no provider credential resolves".to_string())
    }
}
```
OpenCode's version swaps the pure classifier for `opencode_configured_provider_count`
(ANSI-strip + sum count lines, RESEARCH Pattern 3) but keeps the SAME split: a `Command::new`
wrapper method plus a pure, spawn-free classifier function underneath it. Also note Pi's
security discipline worth copying: the error string never echoes raw provider output (see
`pi.rs:114-116`, `"no provider credential resolves"` — a fixed string, not the parsed stdout) —
same posture as RESEARCH's Security Domain note.

**`capabilities` fail-closed probe pattern (D-10)** — two equally-valid analogs exist for the
process-spawn shape; RESEARCH recommends Hermes's `_with(output_fn)` split
(`hermes.rs:74-94`) over Pi's bare spawn (`pi.rs:172-183`) specifically because it avoids a real
subprocess in unit tests:
```rust
// hermes.rs:71-94
pub fn hermes_subagent_dispatch_available() -> bool {
    hermes_subagent_dispatch_available_with(|| {
        std::process::Command::new("hermes")
            .args(["tools", "list"])
            .output()
    })
}

pub fn hermes_subagent_dispatch_available_with(
    output_fn: impl FnOnce() -> Result<std::process::Output, std::io::Error>,
) -> bool {
    let Ok(output) = output_fn() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_hermes_tools_list_for_delegation(&stdout)
}
```
And Pi's `capabilities` trait-method wiring (`pi.rs:46-52`):
```rust
fn capabilities(&self) -> super::DriverCapabilities {
    super::DriverCapabilities {
        subagent_dispatch: pi_subagent_dispatch_available(),
    }
}
```
Fail-closed pure-parser precedent, Pi's substring match on a vetted package name
(`pi.rs:160-183`) — the discipline to copy is "any probe failure/absence → `false`, never a hard
refuse", not the exact substring:
```rust
fn pi_subagent_dispatch_available() -> bool {
    let Ok(output) = std::process::Command::new("pi")
        .args(["list", "--no-approve"])
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .to_lowercase()
            .contains("@bacnh85/pi-subagent")
}
```

**Test harness pattern** — `PathGuard` (RAII PATH-swap, panic-safe restore) is Pi's own
invention (`pi.rs:287-307`), reused verbatim for a stubbed `opencode` binary:
```rust
struct PathGuard {
    original: Option<std::ffi::OsString>,
}

impl PathGuard {
    fn set(path: &std::path::Path) -> Self {
        let original = std::env::var_os("PATH");
        // SAFETY: held under ENV_MUTEX; no other thread reads/writes PATH.
        unsafe { std::env::set_var("PATH", path) };
        Self { original }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(prev) => unsafe { std::env::set_var("PATH", prev) },
            None => unsafe { std::env::remove_var("PATH") },
        }
    }
}
```
Paired stub-binary writer (`pi.rs:246-269`) and the process-global-PATH serialization mutex
(`pi.rs:192-195`, `ENV_MUTEX`) — copy both, since `cargo test` runs in parallel and `set_var` is
process-wide:
```rust
static ENV_MUTEX: Mutex<()> = Mutex::new(());

fn stub_pi_on_path(body: &str, exit_code: i32) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create stub dir");
    let stub = dir.path().join("pi");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{args}'\necho '{body}'\nexit {exit_code}\n",
        args = dir.path().join("args.txt").display(),
        body = body,
        exit_code = exit_code,
    );
    std::fs::write(&stub, script).expect("write pi stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&stub).expect("stat stub").permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&stub, perms).expect("chmod +x stub");
    }
    dir
}
```
Rename to `stub_opencode_on_path` writing an `opencode` stub instead of `pi`. Every test that uses
it takes the `_guard = ENV_MUTEX.lock().unwrap()` line first (see `pi.rs:339-354` for the full
usage pattern, including reading back `args.txt` to assert the exact spawned argv).

---

### `crates/devflow-core/src/agent_result.rs` (service, streaming — event-stream parser)

**Analog:** `is_codex_event_stream` / `parse_codex_event_result`, same file, lines 728-849
(read in full this session; excerpt below is the complete function, not a partial read).

**Format-detector gate pattern** (lines 728-734):
```rust
pub(crate) fn is_codex_event_stream(events: &[serde_json::Value]) -> bool {
    events.iter().any(|v| {
        v.get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|t| t == "thread.started" || t.starts_with("turn."))
    })
}
```
OpenCode's version (per D-03, RESEARCH Pattern 1) gates on `step_start`/`step_finish` instead of
`thread.started`/`turn.*` — same `.any(...)` shape, different literal set.

**Core parser pattern** (lines 756-849, full function) — the three-part structure to replicate:
torn-tail check first, terminal-event resolution second, marker scan third, `None` on no signal:
```rust
pub(crate) fn parse_codex_event_result(stdout: &str) -> Option<AgentResult> {
    let capture = ParsedCapture::parse(stdout);
    let events = &capture.events;

    if !is_codex_event_stream(events) {
        return None;
    }

    if capture.torn_json_after_last_matching(|_| true) {
        return Some(indeterminate_capture_failure());
    }

    let terminal = events.iter().rev().find(|v| {
        matches!(
            v.get("type").and_then(serde_json::Value::as_str),
            Some("turn.completed") | Some("turn.failed")
        )
    });

    let marker = events.iter().rev().find_map(|v| {
        if v.get("type").and_then(serde_json::Value::as_str) != Some("item.completed") {
            return None;
        }
        let item = v.get("item")?;
        if item.get("type").and_then(serde_json::Value::as_str) != Some("agent_message") {
            return None;
        }
        let text = item.get("text").and_then(serde_json::Value::as_str)?;
        parse_marker_lines(text)
    });

    if let Some(terminal) = terminal
        && terminal.get("type").and_then(serde_json::Value::as_str) == Some("turn.failed")
    {
        let reason = terminal
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "codex turn failed".to_string());

        let mut result = AgentResult {
            status: AgentStatus::Failed,
            exit_code: None,
            reason: Some(reason),
            commits: None,
            summary: None,
            verdict: None,
            decided_by_layer: Some(1),
        };
        if let Some(m) = marker.as_ref() {
            result.exit_code = m.exit_code;
            result.commits = m.commits;
            result.summary = m.summary.clone();
            result.verdict = m.verdict;
        }
        return Some(result);
    }

    if let Some(result) = marker {
        return Some(normalise_stream_marker_provenance(result));
    }

    None
}
```
**Structural divergence to make (not to blindly copy):** OpenCode has NO terminal-status event
pair like `turn.completed`/`turn.failed` (D-03). Do not port the `terminal.find(...)` block
unmodified — RESEARCH Pattern 1's `parse_opencode_event_result` skeleton replaces it with a scan
for ANY `type:"error"` event (not just the last line) as the sole hard-failure signal, keeps the
identical torn-tail-first / marker-scan-third shape, and drops the "copy terminal fields onto a
Failed result" step since OpenCode's `error` event carries no separate marker-shaped fields to
merge in the way Codex's `turn.failed` does. See CONTEXT.md D-03 through D-06 and RESEARCH.md
Pattern 1 for the full adapted function body — it is already fully worked there and should be
copied from RESEARCH.md directly rather than re-derived.

**Shared primitives referenced by both the Codex parser above and the new OpenCode parser** (all
read in full this session, exact current line numbers):
- `ParsedCapture::parse` / `torn_json_after_last_matching` — `agent_result.rs:916-994` (`Self::parse` at 922, `torn_json_after_last_matching` at 984-999, quoted signature: `fn torn_json_after_last_matching(&self, pred: impl Fn(&serde_json::Value) -> bool) -> bool`)
- `indeterminate_capture_failure()` — `agent_result.rs:1822-1835`:
  ```rust
  fn indeterminate_capture_failure() -> AgentResult {
      AgentResult {
          status: AgentStatus::Failed,
          exit_code: None,
          reason: Some(
              "stream capture ends in an unparseable line; the final verdict is indeterminate"
                  .to_string(),
          ),
          commits: None,
          summary: None,
          verdict: None,
          decided_by_layer: Some(1),
      }
  }
  ```
- `normalise_stream_marker_provenance` — `agent_result.rs:1849-1852`:
  ```rust
  fn normalise_stream_marker_provenance(mut result: AgentResult) -> AgentResult {
      result.decided_by_layer = Some(1);
      result
  }
  ```
- `parse_marker_lines` — `agent_result.rs:1873-1901` (reverse-line scan, `DEVFLOW_RESULT:` prefix,
  case-insensitive, edge-corruption-stripped, 4000-char whole-line tail budget). Call this on the
  extracted `part.text` string per D-04, exactly as Codex calls it on `item.text` (line 801).
- `strip_corruption_padding` — `agent_result.rs:462-464`, the workspace's precedent for
  hand-rolled text scrubbing (cited by RESEARCH as the model for the new `strip_ansi_escapes`
  helper in `opencode.rs`, since no `regex` crate exists in this workspace).

**Wiring into `evaluate_layer1`** — current chain, `agent_result.rs:2115-2134` (full function
read this session):
```rust
pub fn evaluate_layer1(project_root: &Path, phase: PhaseId) -> Option<AgentResult> {
    if let Some(timed_out) = parse_idle_timeout_side_channel(project_root, phase) {
        return Some(timed_out);
    }

    let stdout = read_capture(&stdout_path(project_root, phase))?;
    detect_claude_rate_limit(&stdout)
        .map(rate_limited_result)
        .or_else(|| detect_claude_envelope_failure(&stdout))
        .or_else(|| parse_claude_event_result(&stdout))
        .or_else(|| parse_antigravity_event_result(&stdout))
        .or_else(|| parse_devflow_result(&stdout))
        .or_else(|| parse_codex_event_result(&stdout))
        .or_else(|| detect_codex_rate_limit(&stdout).map(rate_limited_result))
}
```
Add exactly one line, `.or_else(|| parse_opencode_event_result(&stdout))`, between the
`parse_codex_event_result` line and the trailing `detect_codex_rate_limit` line — per D-12/D-01
placement rationale (RESEARCH: "keeping OpenCode adjacent to Codex documents the relationship").

---

### `crates/devflow-core/src/agents/mod.rs` (test file — three call sites to update, not an analog target)

These are the exact three existing test bodies this phase's argv/health change breaks (found via
`rg -n` this session, current line numbers):

**1. `drivers_reproduce_legacy_adapter_behavior`** (`mod.rs:219-242`) — OpenCode assertion at
lines 235-241:
```rust
// OpenCode: positional `run <prompt>` + byte-identical legacy prompt.
let (program, args) = OpenCodeDriver.build_command(PhaseId::new(7), "x", &[]);
assert_eq!(program, "opencode");
assert_eq!(args, ["run", "x"]);
assert_eq!(
    OpenCodeDriver.render_prompt(&intent),
    crate::prompt::render_claude_style(&intent)
);
```
`assert_eq!(args, ["run", "x"])` must become `assert_eq!(args, ["run", "x", "--auto", "--format", "json"])` (or equivalent) — the `render_prompt` assertion is untouched (D-02: no change).

**2. `opencode_wraps_prompt_in_run`** (`mod.rs:575-582`):
```rust
#[test]
fn opencode_wraps_prompt_in_run() {
    let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
    let (program, args) =
        driver_for(AgentKind::OpenCode).build_command(PhaseId::new(7), &prompt, &[]);
    assert_eq!(program, "opencode");
    assert_eq!(args, ["run", prompt.as_str()]);
}
```
Same fix: `args` must include the `--auto --format json` tail.

**3. `default_preflight_is_ok_for_built_in_adapters`** (`mod.rs:626-641`):
```rust
/// D-13: `preflight`'s default body is `Ok(())` for every built-in
/// adapter — none of Claude/Codex/OpenCode override it in Phase 17 (no
/// reviewer-set storage exists yet in `state.rs`/`config.rs`, review
/// consensus #6).
#[test]
fn default_preflight_is_ok_for_built_in_adapters() {
    let state = crate::state::State::new(
        PhaseId::new(1),
        AgentKind::Claude,
        crate::mode::Mode::Auto,
        PathBuf::from("/repo"),
    );
    assert!(driver_for(AgentKind::Claude).health(&state).is_ok());
    assert!(driver_for(AgentKind::Codex).health(&state).is_ok());
    assert!(driver_for(AgentKind::OpenCode).health(&state).is_ok());
}
```
Remove only the `driver_for(AgentKind::OpenCode).health(&state).is_ok()` line (and update the doc
comment's "none of Claude/Codex/OpenCode override it" claim, since OpenCode now does) — the
Claude and Codex lines stay, since neither overrides `health` in this phase. This mirrors how
`codex_disables_signing_via_env_others_do_not` (`mod.rs:617-624`) already asserts
`driver_for(AgentKind::OpenCode).environment().is_empty()` unaffected — `environment()` is out of
scope for D-01/D-07/D-10 and needs no change.

---

### `crates/devflow-cli/src/commands.rs` (config, optional D-11 drive-by)

**Analog:** adjacent `cmd_check` entries in the same `doctor_checks()` list, `commands.rs:2304-2328`:
```rust
cmd_check(
    "claude",
    "claude",
    "--version",
    "npm i -g @anthropic-ai/claude-code",
),
cmd_check("codex", "codex", "--version", "npm i -g @openai/codex"),
cmd_check(
    "opencode",
    "opencode",
    "--version",
    "cargo install opencode",
),
cmd_check(
    "pi",
    "pi",
    "--version",
    "Install Pi (see https://github.com/earendil-works/pi-mono)",
),
```
If D-11 is taken as a drive-by, only the fourth string argument (the install hint) changes — e.g.
to something like `"npm i -g opencode-ai"` or a Homebrew-appropriate hint (verify the real
install command before writing it; D-11 only established that `cargo install opencode` is wrong,
not what the correct hint is). Every other `cmd_check(name, binary, "--version", hint)` call in
this list is the pattern to match — 4 positional string args, no struct literal.

## Shared Patterns

### Fail-closed probe discipline
**Source:** `pi.rs:172-183` (`pi_subagent_dispatch_available`), `hermes.rs:74-94`
(`hermes_subagent_dispatch_available` / `_with`)
**Apply to:** `OpenCodeDriver::health`, `OpenCodeDriver::capabilities`
Every probe returns the SAFE default (`Err` for health, `false` for capabilities) on ANY of:
process spawn failure (`Command::output()` returns `Err`), non-zero exit status, or unparseable/
unexpected stdout shape. Never propagate a panic, never hard-refuse a launch over a capability
probe (only `health` may refuse a launch; `capabilities` never does).

### Pure classifier split (spawn-free unit testing)
**Source:** `pi.rs:104-117` (`classify_auth_check`), `hermes.rs:97-109`
(`parse_hermes_tools_list_for_delegation`)
**Apply to:** OpenCode's ANSI-strip + provider-count parser and its subagent-list-line parser —
each spawn-wrapping method (`health`, `capabilities`) should delegate to a pure `fn(&str) -> ...`
(or `fn(&str, bool) -> ...`) that every unit test calls directly, with the process-spawn wrapper
tested separately (once) via the stub-binary harness.

### Marker/provenance/torn-tail primitives (do not reimplement)
**Source:** `agent_result.rs` — `ParsedCapture::parse`/`torn_json_after_last_matching` (916-994),
`parse_marker_lines` (1873-1901), `normalise_stream_marker_provenance` (1849-1852),
`indeterminate_capture_failure` (1822-1835)
**Apply to:** `parse_opencode_event_result` — every stream parser in this file (Claude, Codex,
Antigravity, and now OpenCode) shares these four primitives verbatim; do not write a bespoke
JSONL scanner, marker regex, or provenance check for OpenCode.

### RAII env/PATH test guards
**Source:** `pi.rs:287-332` (`PathGuard`, `EnvGuard`), `pi.rs:192-195` (`ENV_MUTEX`)
**Apply to:** any `opencode.rs` test that stubs the `opencode` binary on `PATH` — copy both guard
structs and the mutex verbatim (rename nothing but the stub binary's own name), since `set_var`
is process-wide and `cargo test` runs in parallel.

## No Analog Found

None — every file this phase touches has a strong, current, directly-cited analog (RESEARCH.md
and CONTEXT.md both name the exact files/functions to model each new piece on).

## Metadata

**Analog search scope:** `crates/devflow-core/src/agents/` (codex.rs, pi.rs, hermes.rs, opencode.rs,
mod.rs), `crates/devflow-core/src/agent_result.rs`, `crates/devflow-cli/src/commands.rs`
**Files scanned:** 7 (all read in full or via targeted non-overlapping offset/limit reads this session)
**Pattern extraction date:** 2026-08-23
