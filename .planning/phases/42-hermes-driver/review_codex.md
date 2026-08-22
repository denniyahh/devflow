## Adversarial Review — Phase 42 (OpenAI CodexLuna)

**Target:** `feature/phase-42` (`crates/devflow-core/src/agents/hermes.rs`, `crates/devflow-core/src/state.rs`, `crates/devflow-cli/src/preflight.rs`, `crates/devflow-cli/tests/phase7_cli.rs`, and phase planning artifacts)

---

### Critical Findings

1. **Unattended-mode unlock permits invalid legacy configuration combinations**
   - **File:** `crates/devflow-cli/src/preflight.rs:981-985`
   - **Issue:** `State { agent: Antigravity, legacy_claude_launch: true, mode: Auto }` receives `ConditionState::Holds` because `stream_launch_enabled` only filters out `legacy_opt_out` when `agent == Claude`. The condition allowed `--legacy-claude-launch` to pass for Antigravity without refusal.

2. **Phase verification claims clean quiet gaps without raw cadence data**
   - **Files:** `42-VERIFICATION.md:18-25`, `42-UAT.md:18-54`
   - **Issue:** Claimed zero idle timeouts were observed, while runtime logs documented two 120s watchdog kills during workspace testing. Requires explicit documentation of the 300s timeout remediation.

---

### Warning Findings

3. **Subagent toolset parser uses substring co-occurrence matching**
   - **File:** `crates/devflow-core/src/agents/hermes.rs:97-101`
   - **Issue:** `lower.contains("delegation") && lower.contains("enabled")` returns `true` for disabled lines mentioning enabled tools.
   - **Remediation:** Tokenize or check that `disabled` / `not enabled` does not match the delegation entry.

4. **Synchronous subprocess probes in Hermes driver without timeout**
   - **File:** `crates/devflow-core/src/agents/hermes.rs:58-60, 76-79`
   - **Issue:** `Command::new("hermes").output()` has no timeout wrapper.

---

### Info Findings

5. **Shared conformance suite does not check flag vectors directly**
   - **File:** `crates/devflow-core/src/agents/mod.rs:146-169`
   - **Issue:** Conformance test verifies presence of `DEVFLOW_RESULT` and non-empty program name, leaving specific argv validation to unit tests.

6. **Hung-process test verification boundary**
   - **File:** `crates/devflow-cli/tests/phase7_cli.rs:1916-1930`
   - **Issue:** Integration test relies on external test-harness kill to verify PID tracking rather than an autonomous watchdog.
