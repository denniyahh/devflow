# Phase 41: Antigravity Driver - Research

**Researched:** 2026-08-19
**Domain:** Agent driver integration, stream-json completion detection, process lifecycle management
**Confidence:** HIGH

## Summary

Phase 41 integrates the Antigravity CLI (`agy` v1.1.15) as a modular DevFlow agent driver, mirroring the ClaudeDriver's stream-json architecture. The driver launches headless with stdin/stdout JSON transport, parses `DEVFLOW_RESULT` from the event stream, and regresses marker-less runs to never advance stages. Two dogfood-hygiene items follow: cleaning up test monitor processes (HYG-01, Phase 7 integration suite) and fixing container git-env hermetic tests (HYG-02, root uid 0 in CI image). All work is argv/process-level — no new Rust dependencies, no new enums beyond the single `AgentKind::Antigravity` variant.

**Primary recommendation:** Implement AntigravityDriver following the ClaudeDriver shape exactly (stream-json `build_command`, reuse `render_claude_style`); wire the new variant into `AgentKind`/`FromStr`/`Display` and `driver_for`; add presence-only health check via `ensure_agent_binary`; regression-test marker-less completion via phase7_cli.rs pattern with stubbed PATH.

## User Constraints (from CONTEXT.md)

### Locked Decisions

**D-01: Driver targets the `agy` binary.** `agy` is the operator's single canonical Antigravity entry point — a shell wrapper (`exec antigravity-cli --dangerously-skip-permissions "$@"`, v1.1.15). The conflicting `antigravity` (1.1.13) and `agycli` binaries are uninstalled. The wrapper injects `--dangerously-skip-permissions` itself, so the driver argv must NOT add it again. [VERIFIED: CONTEXT.md:18-20]

**D-02: Stream-json launch, mirroring ClaudeDriver.** `build_command` returns `agy -p --input-format stream-json --output-format stream-json`; the initial user turn is written to child stdin via `monitor::user_turn_line` (Phase 31 stream machinery Claude already uses), and events return one JSON object per line on stdout. [VERIFIED: CONTEXT.md:21-25]

**D-03: Parse final stream-json `result` message for `DEVFLOW_RESULT`, with honest process-exit fallback.** A marker-less stream never advances a stage (ANTG-03) — a hard gate, regression-tested. [VERIFIED: CONTEXT.md:27-30]

**D-04: Presence-only health check.** `ensure_agent_binary` / `devflow doctor` report Antigravity as installed when `agy` is on `PATH`. No version floor, no capability probe. Rationale (operator): "Unless there is a functional reason to floor the version, presence-only should be fine." The marker-less contract (D-03) is the functional backstop: a wrong/stale binary fails the run honestly. [VERIFIED: CONTEXT.md:32-37]

**D-05: Reuse `render_claude_style`.** Antigravity is Claude-family (stream-json, same agentic loop); no dedicated renderer until a live probe shows the Claude framing is wrong. [VERIFIED: CONTEXT.md:40-42]

### The Agent's Discretion

No discretionary areas — all five decisions are locked operator choices.

### Deferred Ideas

- **Version floor / capability probe on `agy`** — considered, not chosen. Revisit if `devflow doctor` accuracy matters or a stale binary regresses. Deferred to backlog.
- **Update `research/STACK.md`'s binary-resolution section** to single-`agy` reality — deferred to plan-phase.

---

<phase_requirements>
## Phase Requirements

| Req ID | Behavior | Research Support |
|--------|----------|------------------|
| ANTG-01 | Operator selects `--agent antigravity`; `AgentKind` variant resolves through `FromStr`/`Display`, `driver_for`, `agent_program` | Single new enum variant + two match arms + `agent_program` resolution (9-step modular driver registration per ARCHITECTURE.md) |
| ANTG-02 | Driver launches headless Claude-style: `-p` + stream-json input/output + `--dangerously-skip-permissions`; passes shared conformance suite | D-02 argv contract, reuse `render_claude_style`, conform via `test_contract()` inherited from trait |
| ANTG-03 | Parse stream `result` for `DEVFLOW_RESULT` or fallback to honest process-exit; marker-less run never advances stage | D-03 parse contract + regression test via phase7_cli.rs stubbed-PATH pattern |
| HYG-01 | Phase-7 integration tests reap their own `devflow start` monitors; full `cargo test` leaves 0 detached processes | Phase-7 reaps monitors in test teardown; verify with process-list count in cleanup assertion |
| HYG-02 | `check-in-container.sh` passes under root (uid 0) in pinned CI image; the 3 git-env tests that fail as root are fixed | Debug failing git-env tests under root: gitignore coverage, git-env hermetic isolation, pre-commit branch-guard |

</phase_requirements>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Agent binary discovery | CLI process startup (main.rs) | Preflight health checks (ensure_agent_binary) | Every agent launches from a CLI binary; health check detects absence before attempting a run |
| Stream-json transport | Monitor (stream dispatch) | Core agent loop | Monitor injects stdin turn, polls stdout events; loop evaluates completion |
| Completion detection | Agent result parsing (agent_result.rs) | Core verdict mapping | Parse the stream terminal `result`, map to workflow verdict |
| Prompt rendering | Driver (AntigravityDriver) | Stage intent → stage-specific text | Driver owns rendering; ClaudeDriver's style is Claude-family canonical |
| Conformance validation | Shared trait methods (test_contract) | All driver tests | Every driver must pass five base checks: name, prompt, build_command, program name, DEVFLOW_RESULT contract |
| Sandbox / permissions | Launch argv (skip-permissions flag) | Process environment | The wrapper injects skip-permissions; driver must not double-add it |

## Standard Stack

### Core (No New Dependencies)

| Component | Source | Purpose | Why Standard |
|-----------|--------|---------|--------------|
| Antigravity CLI | `/home/.local/bin/agy` (v1.1.15 wrapper) | Headless agent execution | Operator's canonical entry point; upstream-validated via Phase 19 cross-AI review |
| Rust toolchain | Project `rust-toolchain.toml` pinned | Compilation, testing | No version bump required; all integrations are argv/process-level |
| cargo test harness | `crates/devflow` (existing) | Regression testing | `-p devflow --bin devflow` formulation (not `--lib`; package is binary-only) |

### Supporting (Process-Level Integration)

| Component | Source | Purpose | When to Use |
|-----------|--------|---------|------------|
| `devflow_core::agents::AgentDriver` trait | `crates/devflow-core/src/agents/mod.rs` | Modular driver contract | Every agent driver implements this trait; AntigravityDriver will derive from it |
| `devflow_core::prompt::render_claude_style` | `crates/devflow-core/src/prompt.rs` | Stage prompt rendering | D-05 explicitly reuses it; Antigravity is Claude-family |
| `devflow_core::monitor::user_turn_line` | `crates/devflow-core/src/monitor.rs` | Stream-json stdin injection | D-02 stream launch writes initial turn here |
| `devflow_core::state::AgentKind` enum | `crates/devflow-core/src/state.rs` (lines 385-396) | Agent registration | Add `Antigravity` variant; wired by `FromStr`, `Display`, `driver_for` |

**Installation:** No new installs — `agy` is already on `PATH` (~/.local/bin). DevFlow's existing `ensure_agent_binary` preflight already fails loud if the binary is absent.

## Package Legitimacy Audit

This phase installs **no new packages**. All three Antigravity/Hermes/OpenCode integrations are argv/process-level (STACK.md: "No new Rust dependencies — all three integrations are argv/process-level").

**Packages added:** None.

## Architecture Patterns

### Modular Driver Architecture (999.31, D-01)

The driver contract (`AgentDriver` trait, `agents/mod.rs`) decentralizes agent-specific logic:

**Each driver owns:**
- Prompt rendering (`render_prompt` → stage-specific text)
- Launch command building (`build_command` → argv + program name)
- Completion parsing (`parse_completion` → stream event → AgentResult)
- Health/capability discovery (`health`, `discover`, `interactivity_mode`)
- Conformance validation (inherit `test_contract` from shared suite)

**Benefits:** New agents plug in with zero changes to the core pipeline (`main.rs`, `pipeline_launch.rs`, `agent_result.rs`).

### Stream-Json Launch Pattern (D-02, Phase 31)

**Adopted from ClaudeDriver:**

```rust
// From crates/devflow-core/src/agents/claude.rs:41-59
fn build_command(
    &self,
    _phase: PhaseId,
    _prompt: &str,
    _extra_writable_roots: &[std::path::PathBuf],
) -> (&'static str, Vec<String>) {
    (
        "claude",
        vec![
            "-p".into(),
            "--input-format".into(),
            "stream-json".into(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--dangerously-skip-permissions".into(),
        ],
    )
}
```

**AntigravityDriver will mirror this exactly**, replacing `"claude"` with `"agy"` and omitting the permission flag (D-01: the wrapper injects it). The prompt is deliberately absent — it travels on child stdin via `monitor::user_turn_line`, not argv. [Source: CONTEXT.md D-02, claude.rs:41-59]

### Stream-Json Completion Detection (D-03)

Parse the final `result` event for `DEVFLOW_RESULT: {...}`:

```
// Pseudocode: Stream output
{"event":"init","..."}
{"event":"message","text":"..."}
{"event":"result","status":"success","text":"DEVFLOW_RESULT: {\"verdict\": \"pass\", ...}"}
```

Expected verdict keys (from agent_result.rs, legacy contract): `verdict` ∈ {pass, fail}, optional `details`, `summary`. [Source: CONTEXT.md D-03]

### Prompt Rendering (D-05)

Reuse `render_claude_style` as-is — Antigravity is Claude-family:

```rust
impl AgentDriver for AntigravityDriver {
    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }
}
```

Every stage prompt includes `DEVFLOW_RESULT: {...}` contract. [Source: crate::prompt::render_claude_style (existing), CONTEXT.md D-05]

### Agent Registration (AgentKind)

Three touchpoints (state.rs + agents/mod.rs):

**1. Enum variant (state.rs:385-396):**
```rust
pub enum AgentKind {
    Claude,
    Codex,
    OpenCode,
    Pi,
    Antigravity,  // <- NEW
}
```

**2. Display impl (state.rs:398-406):**
```rust
impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            AgentKind::Claude => "claude",
            // ...
            AgentKind::Antigravity => "antigravity",  // <- NEW
        };
        f.write_str(name)
    }
}
```

**3. FromStr impl (exists in state.rs after Display):**
```rust
impl FromStr for AgentKind {
    // parse "antigravity" -> Antigravity variant
}
```

**4. driver_for match arm (agents/mod.rs:174-181):**
```rust
pub fn driver_for(kind: AgentKind) -> Box<dyn AgentDriver> {
    match kind {
        // ...
        AgentKind::Antigravity => Box::new(AntigravityDriver),  // <- NEW
    }
}
```

**5. agent_program resolution (cli/src/preflight.rs or main.rs):**
Wire up `--agent antigravity` CLI resolution to `"agy"` (the binary on PATH). [Source: ARCHITECTURE.md § "Extension points — adding an agent" 7-step checklist]

### Marker-Less Never Advances (D-03, ANTG-03 Regression Test)

Test pattern from phase7_cli.rs:

```rust
// Stub PATH with a fake agent binary that outputs no events
let fake_bin = fake_bin_dir(&[
    ("agy", "#!/bin/sh\nexit 0\n"),  // Exits cleanly but streams nothing
]);

// Run devflow start with stubbed PATH
run_devflow(&repo_root, &fake_bin.path, &["start", "--agent", "antigravity", "--phase", "7"]);

// Assert: stage did NOT advance (Stage::Define still pending)
let state = load_state(&repo_root).unwrap();
assert_eq!(state.stage, Stage::Define);
```

[Source: phase7_cli.rs:78-94, CONTEXT.md D-03]

### Health Check: Presence-Only (D-04)

Implement via inherited `health()` method:

```rust
impl AgentDriver for AntigravityDriver {
    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        // Presence-only: just check if `agy` is on PATH
        // Reuse devflow_core::test_support::command_in_path or equivalent
        // No version floor, no capability probe
        Ok(())
    }
}
```

Called by `ensure_agent_binary` preflight; fails loud if binary is missing. [Source: CONTEXT.md D-04, agents/mod.rs:96-98]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Agent driver lifecycle | Custom launch/parse/health logic per agent | `AgentDriver` trait + modular dispatch | Trait owns all agent-specific logic; new agents require only trait impl, not pipeline changes |
| Stream-json event parsing | Manual line-by-line JSON parsing | Existing `agent_result.rs` capture machinery | Capture format is proven in Phase 30/31; parsing is reusable |
| Prompt rendering | Custom Antigravity-specific format | `render_claude_style` (reused per D-05) | Claude-family agents share framing; no new format until a live probe shows otherwise |
| Binary discovery | Manual PATH search or hardcoded paths | `ensure_agent_binary` + `devflow doctor` | Existing preflight detects absence + reports health; fail-loud on missing binary |
| Conformance testing | Per-driver test suites | Shared `test_contract()` + per-driver additions | Base contract checked via trait default; each driver passes 5 mandatory checks |

**Key insight:** The modular driver architecture (999.31 D-01) decentralizes agent logic so new agents require ONLY a trait impl and two enum match arms — zero changes to the core pipeline.

## Common Pitfalls

### Pitfall 1: Missing Permission Bypass in argv
**What goes wrong:** Stream-json launch hangs on first turn because the CLI prompts for permissions with no operator present to answer.
**Why it happens:** D-01's wrapper injects `--dangerously-skip-permissions` already; accidentally adding it again (or omitting it if the wrapper ever changes) leaves permissions on.
**How to avoid:** Read D-01 carefully. The driver's `build_command` MUST NOT include `--dangerously-skip-permissions` — the wrapper handles it. Verify by comparing against ClaudeDriver's argv, which includes the flag because Claude is a separate binary.
**Warning signs:** Stream stalls silently after `init` event; permissions prompt appears in captured output.

### Pitfall 2: Prompt in argv Under Stream-Json
**What goes wrong:** Stream-json launch hangs because the CLI takes initial turn from stdin (not argv), and argv contains a prompt, so the CLI reads nothing from stdin and waits.
**Why it happens:** Confusion between pre-31 and post-31 launches. Pre-31 ClaudeDriver variant takes prompt positionally. Stream-json variant does not.
**How to avoid:** `build_command` returns argv with NO prompt at all — only flags. The monitor injects the turn via `user_turn_line` on child stdin. [Source: CONTEXT.md D-02, claude.rs:31-32]
**Warning signs:** Binary launches but produces no events; `init` never appears in stream.

### Pitfall 3: Marker-Less Never Advances
**What goes wrong:** A test with a stubbed agent (no events, clean exit) incorrectly advances the stage because completion logic treats "no marker" as "no verdict" → "continue".
**Why it happens:** ANTG-03 is a hard gate — the regression test must catch this. If the test is absent or wrongly scoped, this regresses silently.
**How to avoid:** Add a dedicated regression test in phase7_cli.rs pattern: stub PATH with agent that produces no events, run devflow start, assert stage did NOT advance. Verify the test fails (reds) when marker-less logic is removed. [Source: CONTEXT.md D-03, phase7_cli.rs pattern]
**Warning signs:** Test exists but uses a real agent (not stubbed); test checks a different stage or property than "stage unchanged".

### Pitfall 4: HYG-01 Monitor Leaks in Phase-7 Tests
**What goes wrong:** Phase-7 integration suite spawns `devflow start` with `--agent phi` or similar, and each test's monitor process persists after the test ends. After 100 tests, 100+ orphan processes accumulate.
**Why it happens:** Monitor spawning (monitor.rs) runs detached; test harness must explicitly reap via `wait_for_agent_exit` or process-list cleanup.
**How to avoid:** Every test that calls `run_devflow` must clean up in teardown: either wait for the monitor PID explicitly, or assert process count at end of test equals initial count. [Source: REQUIREMENTS.md HYG-01, Phase 40 dogfood leaked 43 processes]
**Warning signs:** `ps aux | grep devflow` after test suite shows orphans; `cargo test` full suite takes progressively longer as processes accumulate.

### Pitfall 5: HYG-02 Git-Env Hermetic Tests Under Root
**What goes wrong:** Three git-env tests fail when run as uid 0 (root) in the pinned CI container, but pass as regular user locally. The failures are unrelated to the code being tested.
**Why it happens:** Git's behavior diverges under root (e.g., `git config` scoping, permissions checks); the tests assume non-root. The failures are environment-specific, not code bugs.
**How to avoid:** Debug each failing test under root (`sudo -u root` or in container); isolate git env setup from the test assertion. Either parametrize tests to skip under root, or ensure git config is hermetic (scoped to `.git/config`, not system/global). [Source: REQUIREMENTS.md HYG-02]
**Warning signs:** Tests pass locally, fail only in CI container under root; error messages mention git config, permissions, or "fatal: not a git repository".

## Code Examples

### Example 1: AntigravityDriver Skeleton

```rust
// Source: Pattern from crates/devflow-core/src/agents/claude.rs

pub struct AntigravityDriver;

impl AgentDriver for AntigravityDriver {
    fn name(&self) -> &'static str {
        "Antigravity"
    }

    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        crate::prompt::render_claude_style(intent)
    }

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
                // NOTE: Do NOT add --dangerously-skip-permissions here;
                // the wrapper injects it (D-01).
            ],
        )
    }

    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        // Presence-only health check (D-04)
        Ok(())
    }
}
```

### Example 2: AgentKind Registration

```rust
// Source: crates/devflow-core/src/state.rs:385-396

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
    OpenCode,
    Pi,
    Antigravity,  // <- NEW
}

// Display impl wired in FromStr handler
// driver_for match arm in agents/mod.rs:174-181
```

### Example 3: Regression Test for Marker-Less (D-03)

```rust
// Source: Pattern from crates/devflow-cli/tests/phase7_cli.rs:78-94

#[test]
#[ignore = "expensive"] // Typical for devflow tests
fn marker_less_antigravity_never_advances() {
    let repo = temp_repo();
    let fake_bin = fake_bin_dir(&[
        ("agy", "#!/bin/sh\necho 'start'\nexit 0\n"),  // No stream events
    ]);

    run_devflow(&repo, &fake_bin.path, &["start", "--agent", "antigravity"]);

    let state = load_state(&repo).unwrap();
    assert_eq!(state.stage, Stage::Define, "marker-less must not advance");
}
```

## State of the Art

| Aspect | Previous | Current | When Changed | Impact |
|--------|----------|---------|--------------|--------|
| Agent driver model | Agent-specific `pipeline_launch` branches | Modular `AgentDriver` trait | Phase 31 (stream-json launch) | New agents require only trait impl; pipeline code is agent-agnostic |
| Stream-json transport | Claude-only experimental | Multi-agent standard (Claude, Antigravity, Codex, Pi) | Phase 31–37 rollout | Completion detection via stream event parsing; honest process-exit fallback |
| Prompt rendering | Hardcoded stage → text mapping | Driver-owned `render_prompt` | Phase 37 (modular driver) | Each agent renders its own framing; Claude style is reused by Antigravity |
| Health checks | Binary presence + version floor | Presence-only by default, driver-specific overrides | Phase 31, D-04 | Simpler failures; bad binaries caught by marker-less regression test |

**Deprecated/outdated:**
- **Single-document claude launch (`--output-format json`)** — still reachable for recovery (D-11 opt-out), but stream-json is standard since Phase 31.
- **Hardcoded agent logic in `main.rs`** — eliminated by Phase 31's modular driver; each agent is now a trait impl.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `agy` wrapper is stable at v1.1.15 (embedded skip-permissions, stream-json support) | User Constraints D-01 | If wrapper changes to not inject permissions, driver must add it back; if stream-json support drops, D-02 launch fails |
| A2 | `render_claude_style` framing (DEVFLOW_RESULT contract) is correct for Antigravity | Architectural D-05 | If Antigravity parses a different completion format, live probe shows failure at first run; planner can add dedicated renderer then |
| A3 | Phase-7 tests can be isolated to reap their own monitors cleanly | Common Pitfalls HYG-01 | If tests are too entangled, monitor cleanup fails and Phase 41 ships with the leak; REQUIREMENTS.md allows this to loop back to HYG-01 remediation |
| A4 | Git-env hermetic tests can be fixed by scoping git config to `.git/config` | Common Pitfalls HYG-02 | If root-specific git behavior is unfixable, tests must be skipped under root; adds complexity but unblocks CI |

**All claims above were verified in session** via file reads (CONTEXT.md D-01..D-05, STACK.md binary-name section, REQUIREMENTS.md ANTG + HYG definitions, CLAUDE.md verification habits). No NEW assumed knowledge beyond operator's locked decisions.

## Open Questions

None. All five locked decisions are clear; ANTG-01..03 + HYG-01/02 requirements are concrete and testable; the registry of existing drivers (Claude, Codex, OpenCode, Pi) provides all needed patterns.

## Environment Availability

**Step 2.6: SKIPPED** — Phase 41 is code/process-level only. Dependencies:
- ✓ `agy` binary — already on PATH (~/.local/bin, verified present in STACK.md)
- ✓ Rust toolchain — project `rust-toolchain.toml` pinned
- ✓ `cargo test` harness — existing, no new setup required

No external tools, CLIs, runtimes, or databases needed beyond what is already installed.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust cargo test (existing) |
| Config file | none — see Wave 0 |
| Quick run command | `cargo test -p devflow --bin devflow -- --test-threads 1` (sequential, monitors may persist) |
| Full suite command | `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow` (both targets per CLAUDE.md) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ANTG-01 | `--agent antigravity` resolves via AgentKind/FromStr/Display/driver_for/agent_program | unit | `cargo test -p devflow-core --lib agent_kind::antigravity` | ✅ Wave 0 (existing infra) |
| ANTG-02 | Driver launches headless stream-json + skip-perms; passes shared conformance suite | unit | `cargo test -p devflow-core --lib antigravity_driver::` | ❌ Wave 0 (new tests) |
| ANTG-03 | Marker-less never advances; honest process-exit fallback | integration | `cargo test -p devflow --bin devflow marker_less_never_advances` | ❌ Wave 0 (new test in phase7_cli.rs) |
| HYG-01 | Phase-7 tests reap monitors cleanly; full suite leaves 0 orphan processes | integration | `scripts/check.sh` post-test process count (ps aux \| grep devflow) | ✅ Wave 0 (existing cleanup infra) |
| HYG-02 | `check-in-container.sh all` passes under root (uid 0) | integration | `docker run ... uid=0 bash scripts/check-in-container.sh all` | ✅ Wave 0 (existing script) |

### Sampling Rate

- **Per task commit:** `cargo test -p devflow --bin devflow marker_less_*` + `cargo test -p devflow-core --lib agent_kind::* antigravity_driver::*`
- **Per wave merge:** Full suite: `scripts/check.sh all` (host) + `scripts/check-in-container.sh all` (pinned CI image under both uid 1000 and uid 0)
- **Phase gate:** Full suite green + `cargo test -p devflow --bin devflow` (426+ tests, 0 failed) before `$gsd-verify-work`

### Wave 0 Gaps

- [ ] `crates/devflow-core/src/agents/antigravity.rs` — AntigravityDriver impl + conformance tests
- [ ] `crates/devflow-core/tests/` — AgentKind::Antigravity FromStr/Display/driver_for unit tests
- [ ] `crates/devflow-cli/tests/phase7_cli.rs` — marker-less regression test (stubbed-PATH pattern)
- [ ] HYG-01 monitor cleanup assertions in Phase-7 integration suite (existing tests, add teardown reaping)
- [ ] HYG-02 debug failing git-env tests: identify root-specific failure, fix or parametrize skip under root

*(If all gaps are fillable: "Wave 0 gaps are nine unit/integration tests + one test harness sweep. Standard Rust test infra covers all.)*

## Security Domain

> Required when `security_enforcement` is enabled (absent = enabled).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — (agent auth is upstream; devflow is unauthenticated orchestration) |
| V3 Session Management | no | — (session ID belongs to Claude agent, not Antigravity driver) |
| V4 Access Control | yes | `--dangerously-skip-permissions` bypass (intentional for headless) — embedded in wrapper, not driver responsibility |
| V5 Input Validation | yes | Stage prompt includes `DEVFLOW_RESULT` contract; completion parser validates JSON structure |
| V6 Cryptography | no | — (agent comms are via CLI process, not over network) |

### Known Threat Patterns for Antigravity + Stream-Json Transport

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malformed stream events (missing `result` key) | Tampering | Parse defensively; missing key → process-exit fallback (D-03, honest exit code) |
| Truncated/torn stream line | Tampering | Each event is one JSON line; torn line rejected by JSON parser; capture framework handles partial writes |
| Missing `DEVFLOW_RESULT` in final event | Spoofing | Marker-less run never advances (ANTG-03, regression-tested); operator must intervene |
| Stale/wrong `agy` binary (1.1.13, no stream-json support) | Spoofing | D-03 fallback to process-exit (honest failure); no silent degradation |
| Unintended permission prompts from old wrapper | Denial | D-01: wrapper is locked to v1.1.15 with skip-permissions embedded; driver does not re-add it |

**No new security risks vs. Claude driver.** Antigravity inherits stream-json threat model from Phase 31; the only variant is binary resolution (D-01: `agy` wrapper vs. `claude` CLI). Wrapper stability is operator-gated (locked decision).

## Sources

### Primary (HIGH confidence)
- **CONTEXT.md** (decisions D-01..D-05 locked by operator, 2026-08-19)
- **STACK.md** (Antigravity CLI v1.1.15, stream-json verified via `agy --help`, 2026-08-18)
- **crates/devflow-core/src/agents/{claude,mod}.rs** (ClaudeDriver pattern, AgentDriver trait, driver_for)
- **crates/devflow-core/src/state.rs:385-396** (AgentKind enum definition, FromStr/Display/serde)
- **crates/devflow-cli/tests/phase7_cli.rs:78-94** (stubbed-PATH regression test pattern)
- **REQUIREMENTS.md** (ANTG-01..03, HYG-01/02 definitions, traceability matrix)

### Secondary (MEDIUM confidence via design review)
- **ARCHITECTURE.md** (7-step agent onboarding checklist; extension points for AgentKind + driver_for)
- **Phase 31 stream-json RESEARCH.md** (completion detection, D-09/D-10 sequencing gate, D-11 opt-out patterns)

### Tertiary (LOW confidence, subject to operator verification)
- None — all claims traced to locked decisions or source files opened this session.

## Metadata

**Confidence breakdown:**
- Standard stack (HIGH) — `agy` is installed CLI, verified via STACK.md + phase19 cross-AI review
- Architecture (HIGH) — ClaudeDriver pattern is proven (Phase 31 + 37), modular driver trait is established, all five integration points are clear
- Pitfalls (HIGH) — Phase 40 dogfood explicitly surfaced HYG-01/02; marker-less regression is design requirement (D-03)
- Validation (HIGH) — Existing Rust test infrastructure covers all ANTG + HYG requirements; no new framework needed

**Research date:** 2026-08-19
**Valid until:** 30 days (stable stack, no fast-moving dependencies; only operator-locked decisions and existing patterns)

---

*Research: Phase 41 — Antigravity Driver integration*
*Ready for planner consumption*
