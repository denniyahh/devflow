# Phase 42: Hermes Driver - Pattern Map

**Mapped:** 2026-08-21
**Files analyzed:** 6
**Analogs found:** 6 / 6 (all with strong matches)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/devflow-core/src/agents/hermes.rs` | agent driver component | process-exit transport | `crates/devflow-core/src/agents/pi.rs` | exact |
| `crates/devflow-core/src/state.rs` (AgentKind variant) | enum variant + Display/FromStr | serialization | `crates/devflow-core/src/state.rs:385-427` | exact |
| `crates/devflow-core/src/agents/mod.rs` (driver_for arm + conformance) | registry match arm + test | dispatch | `crates/devflow-core/src/agents/mod.rs:175-181` | exact |
| `crates/devflow-cli/src/commands.rs` (doctor check) | diagnostic probe | presence check | `crates/devflow-cli/src/commands.rs:2324-2338` | exact |
| `crates/devflow-cli/src/preflight.rs` (unattended condition) | preflight gate | auto-mode guard | `crates/devflow-cli/src/preflight.rs:981-1009` | exact |
| `crates/devflow-cli/tests/phase7_cli.rs` (integration regressions) | integration tests | stubbed-PATH runner | `crates/devflow-cli/tests/phase7_cli.rs:1320-1435` | exact |

## Pattern Assignments

### `crates/devflow-core/src/agents/hermes.rs` (NEW — agent driver, process-exit)

**Analog:** `crates/devflow-core/src/agents/pi.rs` & `crates/devflow-core/src/agents/antigravity.rs`

**Imports pattern:**
```rust
//! Hermes coding-agent harness adapter (phase 42).
//!
//! Launches `hermes -z "<prompt>" --yolo --accept-hooks` in headless oneshot mode (D-01).
//! `--yolo` bypasses command approvals; `--accept-hooks` prevents interactive TTY prompts.
//! Sets `HERMES_ACCEPT_HOOKS=1` in the environment. Renders prompts via `render_claude_style` (D-02).
//! Probes subagent capabilities via `hermes tools list` looking for enabled delegation (D-04).

use super::AgentDriver;
use crate::phase_id::PhaseId;
use std::path::PathBuf;
```

**Struct and trait impl skeleton:**
```rust
/// The modular driver for Hermes (Phase 42): oneshot `-z` launch with `--yolo` and
/// `--accept-hooks`, standard slash-command prompt rendering via `render_claude_style`,
/// dynamic subagent capability detection, and conformance validation.
pub struct HermesDriver;

impl AgentDriver for HermesDriver {
    fn name(&self) -> &'static str {
        "Hermes"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }

    fn capabilities(&self) -> super::DriverCapabilities {
        super::DriverCapabilities {
            subagent_dispatch: hermes_subagent_dispatch_available(),
        }
    }

    fn build_command(
        &self,
        _phase: PhaseId,
        prompt: &str,
        _extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        (
            "hermes",
            vec![
                "-z".into(),
                prompt.to_string(),
                "--yolo".into(),
                "--accept-hooks".into(),
            ],
        )
    }

    fn environment(&self) -> Vec<(String, String)> {
        vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]
    }

    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        Ok(())
    }
}
```

**Subagent Capability Probe:**
```rust
fn hermes_subagent_dispatch_available() -> bool {
    let Ok(output) = std::process::Command::new("hermes")
        .args(["tools", "list"])
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|l| l.contains("enabled") && l.contains("delegation"))
}
```

### `crates/devflow-core/src/state.rs`

**Analog:** `AgentKind::Antigravity`
- Add `AgentKind::Hermes`
- `Display` maps `AgentKind::Hermes` => `"hermes"`
- `FromStr` maps `"hermes"` => `Ok(AgentKind::Hermes)`
- Error message: `"unsupported agent \`{0}\`; expected claude, codex, opencode, pi, antigravity, or hermes"`

### `crates/devflow-core/src/agents/mod.rs`

**Analog:** `driver_for` match arm & conformance suite
- `driver_for(AgentKind::Hermes) => Box::new(HermesDriver)`
- `every_driver_passes_the_conformance_suite` hardcoded array: 5 → 6 drivers
- Add `hermes_conformance_enrollment` asserting 7 contract checks

### `crates/devflow-cli/src/commands.rs`

**Analog:** `doctor_checks`
```rust
        cmd_check(
            "hermes",
            "hermes",
            "--version",
            "Install Hermes CLI so `hermes` is on PATH",
        ),
```

### `crates/devflow-cli/src/preflight.rs`

**Analog:** `unattended_launch_shape_condition` (C2 gate)
- Updated when ANTG-04 is completed to allow Antigravity in unattended mode.

### `crates/devflow-cli/tests/phase7_cli.rs`

**Analog:** `pi_stub` / `pi_marker_less_run_does_not_advance`
- `hermes_stub(launch: &str)`
- `hermes_marker_less_run_does_not_advance()`
- `hermes_nonzero_exit_does_not_advance()`
- `hermes_hung_process_is_detected_not_left_running()`
