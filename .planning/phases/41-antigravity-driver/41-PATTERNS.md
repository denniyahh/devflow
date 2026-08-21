# Phase 41: Antigravity Driver - Pattern Map

**Mapped:** 2026-08-19
**Files analyzed:** 6
**Analogs found:** 6 / 6 (all with strong matches)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/devflow-core/src/agents/antigravity.rs` | agent driver component | stream-json transport | `crates/devflow-core/src/agents/claude.rs` | exact |
| `crates/devflow-core/src/state.rs` (AgentKind variant) | enum variant + Display/FromStr | serialization | `crates/devflow-core/src/state.rs:385-427` | exact |
| `crates/devflow-core/src/agents/mod.rs` (driver_for arm) | registry match arm | request-response (dispatch) | `crates/devflow-core/src/agents/mod.rs:173-181` | exact |
| `crates/devflow-core/tests/` (AgentKind unit tests) | unit tests | test coverage | `crates/devflow-core/src/state.rs:471-529` | exact |
| `crates/devflow-core/tests/` (AntigravityDriver conformance tests) | unit tests | test coverage | `crates/devflow-core/src/agents/claude.rs:138-276` | exact |
| `crates/devflow-cli/tests/phase7_cli.rs` (marker-less regression test) | integration test | stubbed-PATH agent | `crates/devflow-cli/tests/phase7_cli.rs:78-150` | exact |

## Pattern Assignments

### `crates/devflow-core/src/agents/antigravity.rs` (NEW — agent driver, stream-json)

**Analog:** `crates/devflow-core/src/agents/claude.rs`

**Imports pattern** (lines 1-9):
```rust
//! Antigravity CLI agent driver.
//!
//! Launches `agy -p` headless with a bidirectional `stream-json` transport:
//! the initial user turn travels on the child's **stdin**, and its events come
//! back on stdout one JSON object per line (D-02).

use super::AgentDriver;
use crate::phase_id::PhaseId;
```

**Struct and trait impl skeleton** (lines 11-19):
```rust
/// The modular driver for Antigravity (41-02): owns the `stream-json` launch,
/// reused prompt rendering via `render_claude_style` (D-05), and conformance
/// validation through the shared trait contract.
pub struct AntigravityDriver;

impl AgentDriver for AntigravityDriver {
    fn name(&self) -> &'static str {
        "Antigravity"
    }
```

**Render prompt method** (reuse pattern from D-05):
```rust
    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }
```

**Build command method** (D-02 pattern — stream-json argv, NO prompt in argv):
```rust
    /// Build the headless `stream-json` launch (D-02).
    ///
    /// **The prompt is deliberately absent from the returned argv.** Under
    /// `--input-format stream-json` the CLI takes its initial user turn from
    /// stdin as a JSON document; the monitor writes that turn via
    /// `crate::monitor::user_turn_line`. The `prompt` parameter is kept in the
    /// signature for the shared `AgentDriver` shape — it is unused here on
    /// purpose.
    ///
    /// Note: Do NOT add `--dangerously-skip-permissions` — the `agy` wrapper
    /// (v1.1.15) injects it already (D-01). Adding it again is harmless but
    /// redundant.
    fn build_command(
        &self,
        _phase: PhaseId,
        _prompt: &str,
        _extra_writable_roots: &[std::path::PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "agy",
            vec![
                "-p".into(),
                "--input-format".into(),
                "stream-json".into(),
                "--output-format".into(),
                "stream-json".into(),
            ],
        )
    }
```

**Health method** (D-04 — presence-only):
```rust
    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        // Presence-only health check (D-04): just verify `agy` is on PATH.
        // No version floor, no capability probe. Process-exit failure (D-03)
        // is the functional backstop.
        Ok(())
    }
}
```

**Test structure** (mirror ClaudeDriver tests, lines 138-276):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const PROMPT: &str = "do the work, then emit DEVFLOW_RESULT: {...}";

    #[test]
    fn build_command_uses_stream_json_both_input_output() {
        let (program, args) = AntigravityDriver.build_command(PhaseId::new(7), PROMPT, &[]);
        assert_eq!(program, "agy");
        assert!(args.windows(2).any(|w| w[0] == "--input-format" && w[1] == "stream-json"));
        assert!(args.windows(2).any(|w| w[0] == "--output-format" && w[1] == "stream-json"));
    }

    #[test]
    fn build_command_carries_no_positional_prompt() {
        let (_program, args) = AntigravityDriver.build_command(PhaseId::new(7), PROMPT, &[]);
        assert!(
            !args.iter().any(|arg| arg.contains("DEVFLOW_RESULT")),
            "prompt must travel on stdin, not argv: {args:?}"
        );
    }

    #[test]
    fn render_prompt_includes_devflow_result_contract() {
        let intent = crate::prompt::StageIntent::for_stage(Stage::Code, PhaseId::new(7));
        let prompt = AntigravityDriver.render_prompt(&intent);
        assert!(prompt.contains("DEVFLOW_RESULT"), "prompt must include completion contract");
    }
}
```

---

### `crates/devflow-core/src/state.rs` AgentKind (enum variant + FromStr/Display)

**Analog:** `crates/devflow-core/src/state.rs:385-427`

**Enum variant addition** (lines 385-396):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    /// Anthropic Claude Code CLI.
    Claude,
    /// OpenAI Codex CLI.
    Codex,
    /// OpenCode CLI.
    OpenCode,
    /// Pi coding-agent harness.
    Pi,
    /// Antigravity CLI.
    Antigravity,  // <- NEW (D-01)
}
```

**Display impl pattern** (lines 398-408, add arm for Antigravity):
```rust
impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::OpenCode => "opencode",
            AgentKind::Pi => "pi",
            AgentKind::Antigravity => "antigravity",  // <- NEW
        };
        f.write_str(name)
    }
}
```

**FromStr impl pattern** (lines 410-422, add arm for Antigravity):
```rust
impl FromStr for AgentKind {
    type Err = AgentParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "claude" => Ok(AgentKind::Claude),
            "codex" => Ok(AgentKind::Codex),
            "opencode" | "open-code" => Ok(AgentKind::OpenCode),
            "pi" => Ok(AgentKind::Pi),
            "antigravity" => Ok(AgentKind::Antigravity),  // <- NEW
            other => Err(AgentParseError(other.to_string())),
        }
    }
}
```

**AgentParseError pattern** (lines 424-427, update error message):
```rust
#[derive(Debug, Clone, thiserror::Error)]
#[error("unsupported agent `{0}`; expected claude, codex, opencode, pi, or antigravity")]
pub struct AgentParseError(String);
```

---

### `crates/devflow-core/src/agents/mod.rs` driver_for match arm

**Analog:** `crates/devflow-core/src/agents/mod.rs:173-181`

**Registry lookup function** (lines 173-181, add Antigravity arm):
```rust
/// Return the driver for a configured agent kind.
pub fn driver_for(kind: AgentKind) -> Box<dyn AgentDriver> {
    match kind {
        AgentKind::Claude => Box::new(ClaudeDriver),
        AgentKind::Codex => Box::new(CodexDriver),
        AgentKind::OpenCode => Box::new(OpenCodeDriver),
        AgentKind::Pi => Box::new(PiDriver),
        AgentKind::Antigravity => Box::new(AntigravityDriver),  // <- NEW
    }
}
```

**Module exports** (lines 183-191, add exports for AntigravityDriver):
```rust
pub mod claude;
pub mod codex;
pub mod opencode;
pub mod pi;
pub mod antigravity;  // <- NEW module

pub use claude::ClaudeDriver;
pub use codex::CodexDriver;
pub use opencode::OpenCodeDriver;
pub use pi::PiDriver;
pub use antigravity::AntigravityDriver;  // <- NEW export
```

**Conformance test update** (lines 199-205, add Antigravity assertion):
```rust
#[test]
fn driver_for_returns_correct_names() {
    assert_eq!(driver_for(AgentKind::Claude).name(), "Claude Code");
    assert_eq!(driver_for(AgentKind::Codex).name(), "OpenAI Codex");
    assert_eq!(driver_for(AgentKind::OpenCode).name(), "OpenCode");
    assert_eq!(driver_for(AgentKind::Pi).name(), "Pi");
    assert_eq!(driver_for(AgentKind::Antigravity).name(), "Antigravity");  // <- NEW
}
```

---

### `crates/devflow-core/tests/` — AgentKind unit tests

**Analog:** `crates/devflow-core/src/state.rs:471-529`

**Test structure** (add these in `crates/devflow-core/tests/` or state.rs test module):
```rust
#[cfg(test)]
mod agent_kind_antigravity {
    use super::*;

    #[test]
    fn agent_from_str_accepts_antigravity() {
        assert_eq!("antigravity".parse::<AgentKind>().unwrap(), AgentKind::Antigravity);
        assert_eq!("ANTIGRAVITY".parse::<AgentKind>().unwrap(), AgentKind::Antigravity);
    }

    #[test]
    fn agent_display_renders_antigravity() {
        assert_eq!(AgentKind::Antigravity.to_string(), "antigravity");
    }

    #[test]
    fn agent_kind_serializes_lowercase() {
        let serialized = serde_json::to_string(&AgentKind::Antigravity).unwrap();
        assert_eq!(serialized, "\"antigravity\"");
    }

    #[test]
    fn agent_kind_deserializes_lowercase() {
        let deserialized: AgentKind = serde_json::from_str("\"antigravity\"").unwrap();
        assert_eq!(deserialized, AgentKind::Antigravity);
    }

    #[test]
    fn driver_for_antigravity_returns_correct_instance() {
        let driver = driver_for(AgentKind::Antigravity);
        assert_eq!(driver.name(), "Antigravity");
    }
}
```

---

### `crates/devflow-cli/tests/phase7_cli.rs` — Marker-less regression test

**Analog:** `crates/devflow-cli/tests/phase7_cli.rs:78-150` (fake_bin_dir, run_devflow pattern)

**Regression test structure** (D-03: marker-less never advances):
```rust
#[test]
#[ignore = "expensive"]
fn marker_less_antigravity_never_advances() {
    let repo_root = tempfile::tempdir().unwrap();
    init_repo(repo_root.path());

    // Stub PATH with fake `agy` that produces no stream events and exits cleanly (D-03).
    let fake_bin = fake_bin_dir(&[
        ("agy", "#!/bin/sh\nexit 0\n"),  // No output, clean exit — marker-less
    ]);

    // Run `devflow start --agent antigravity --phase 7` with stubbed PATH.
    let output = run_devflow(repo_root.path(), fake_bin.path(), &[
        "start",
        "--agent", "antigravity",
        "--phase", "7",
    ]);

    // Assert: stage did NOT advance (Stage::Define still pending).
    // A marker-less run with honest process-exit must never advance the stage.
    let state = load_state(repo_root.path()).unwrap();
    assert_eq!(
        state.stage, Stage::Define,
        "marker-less antigravity run must not advance stage: {:?}",
        state.stage
    );
}

#[test]
#[ignore = "expensive"]
fn antigravity_parses_devflow_result_from_stream() {
    // Test that a well-formed stream-json output with DEVFLOW_RESULT
    // advances the stage (happy path for completion detection — D-03).
    let repo_root = tempfile::tempdir().unwrap();
    init_repo(repo_root.path());

    // Stub PATH with fake `agy` that produces a single well-formed stream event.
    let fake_bin = fake_bin_dir(&[
        (
            "agy",
            "#!/bin/sh\necho '{\"event\":\"result\",\"text\":\"DEVFLOW_RESULT: {\\\"verdict\\\": \\\"pass\\\"}\"}'",
        ),
    ]);

    let output = run_devflow(repo_root.path(), fake_bin.path(), &[
        "start",
        "--agent", "antigravity",
        "--phase", "7",
    ]);

    // Assert: stage advanced (DEVFLOW_RESULT parsed successfully).
    let state = load_state(repo_root.path()).unwrap();
    assert_ne!(
        state.stage, Stage::Define,
        "well-formed DEVFLOW_RESULT must advance stage: {:?}",
        state.stage
    );
}
```

**Helper functions (from analog)** — already exist in phase7_cli.rs:
- `fake_bin_dir(scripts: &[(&str, &str)]) -> FakeBin` (lines 78-89)
- `run_devflow(root: &Path, fake_bin: &Path, args: &[&str]) -> Output` (lines 96-149)
- `init_repo(root: &Path)` (lines 35-76)
- `load_state(root: &Path) -> Result<State, _>` (inferred; read from `.devflow/state.json`)

**Integration context** (phase7_cli.rs uses ENV_MUTEX for test isolation):
- Tests run sequentially due to shared git/filesystem state
- `#[ignore = "expensive"]` gates integration tests
- `fake_bin` prepended to PATH overrides real binaries
- `DEVFLOW_TEST_ROOT` env var passed to devflow CLI for fixture isolation

---

## Shared Patterns

### Stream-JSON Launch (D-02)
**Source:** `crates/devflow-core/src/agents/claude.rs:41-59`
**Apply to:** `AntigravityDriver.build_command()`

The stream-json pattern is proven in ClaudeDriver and reused identically for Antigravity:
- Return `("agy", vec!["-p", "--input-format", "stream-json", "--output-format", "stream-json"])`
- NO prompt in argv (travels on stdin via `monitor::user_turn_line`)
- NO prompt parameter used (kept for trait shape)
- Child receives JSON event stream on stdout, one object per line

### Prompt Rendering Reuse (D-05)
**Source:** `crates/devflow-core/src/prompt::render_claude_style()`
**Apply to:** `AntigravityDriver.render_prompt()`

```rust
fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
    crate::prompt::render_claude_style(intent)
}
```

Antigravity is Claude-family (stream-json, same agentic loop) — no dedicated renderer until live probe shows the Claude framing is wrong (D-05).

### Conformance Test Contract (37-04)
**Source:** `crates/devflow-core/src/agents/mod.rs:146-171`
**Apply to:** All AntigravityDriver tests

Every driver trait impl automatically inherits `test_contract()`, which validates:
1. `name()` is non-empty
2. `render_prompt()` includes "DEVFLOW_RESULT" for each stage
3. `build_command()` returns a non-empty program name

AntigravityDriver must pass all five checks by inheriting the trait default.

### Agent Registration (AgentKind)
**Source:** `crates/devflow-core/src/state.rs:385-427`
**Apply to:** All three touchpoints (enum, Display, FromStr)

Pattern: add one variant to enum, one match arm per impl (Display, FromStr), update error message. Serde derives handle serialization automatically via `#[serde(rename_all = "lowercase")]`.

### Driver Registry (driver_for)
**Source:** `crates/devflow-core/src/agents/mod.rs:173-181`
**Apply to:** driver_for match + module exports + conformance test

Pattern: add match arm, add module import, add pub re-export, add assertion in conformance test. No other changes needed — the pipeline is agent-agnostic via the trait.

### Marker-Less Never Advances Regression (D-03, ANTG-03)
**Source:** `crates/devflow-cli/tests/phase7_cli.rs:78-150` pattern
**Apply to:** phase7_cli.rs test addition

Pattern: stub PATH with fake agent, run devflow, assert Stage::Define unchanged. This is a **hard gate** (ANTG-03 requirement) — the test must fail (red) when marker-less logic is removed.

---

## No Analog Found

None — all files have direct precedents in the codebase.

---

## Metadata

**Analog search scope:** `crates/devflow-core/src/agents/`, `crates/devflow-core/src/state.rs`, `crates/devflow-cli/tests/`

**Files scanned:** 4 primary (claude.rs, mod.rs, state.rs, phase7_cli.rs) + supporting (pi.rs, monitor.rs)

**Pattern extraction date:** 2026-08-19

**Key insights:**
1. AntigravityDriver is a drop-in analog of ClaudeDriver (stream-json, same argv shape, reused prompt rendering)
2. AgentKind registration is a 3-point pattern: enum variant + Display + FromStr (serde handle serialization)
3. driver_for is a simple match arm + module export (no pipeline changes needed)
4. Marker-less regression test follows the stubbed-PATH pattern already proven in phase7_cli.rs
5. All conformance tests inherit the shared `test_contract()` — no new test infrastructure required
