gsd-tools: warning: /home/denniyahh/.gsd/defaults.json sets model_profile, commit_docs, parallelization, resolve_model_ids, granularity, runtime but a project config takes precedence here — those global keys are ignored for model resolution. (#3532)
# Adversarial Code Review — Phase 42 (Hermes Driver & Antigravity Auto-Mode Graduation)

I read the implementation, the two plans, and the evidence files directly. No praise; findings only.

---

## Finding 1 — Warning (High): "Hung process handled cleanly" is unimplemented, and the test that purports to prove cleanup proves the opposite

**File:** `crates/devflow-core/src/monitor.rs:489` (legacy arm) and `crates/devflow-cli/tests/phase7_cli.rs:1877-1928`

**Quoted snippet (monitor.rs):**
```rust
         wait $apid; echo $? > {exit_file}{advance_tail}",
```

The Legacy `sh` monitor's `wait $apid` blocks **with no timeout**. The idle-timeout machinery (`IdleTimeoutSetting`, `DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`) is wired into the `PipeOwning` arm only (`monitor.rs:1125`: *"Scoped to the `PipeOwning` arm alone: `Legacy` keeps today's behaviour"*). Hermes is not a stream agent (`stream_launch_enabled` matches only `Claude | Antigravity`), so every Hermes launch goes through this arm. A hung `hermes` therefore hangs the monitor **indefinitely** — nothing sends it a signal, nothing times out, nothing reaps.

The test is worse than absent — it's inverted. `hermes_hung_process_is_detected_not_left_running` (`phase7_cli.rs:1877`) does:

```rust
// line 1907
"the hung hermes process must still be alive"   // asserts DevFlow did NOT kill it
...
// line 1922
"should be able to kill hung hermes process"     // the TEST kills it, via Command::new("kill")
```

The test's own body kills the process; DevFlow never does. So the plan's **must_have** — *"hung `hermes` processes are handled cleanly without advancing stages or leaking processes (HRMS-03)"* — is not met and not tested. The test proves a hung Hermes is left running until an *external* `kill`, i.e. it is leaked by default.

**Failure mode:** a real `hermes -z` that wedges (model hang, network stall) leaves the monitor in `wait` forever; `devflow status` reports the stage in-flight with no timeout, and no automatic recovery ever fires. Exactly the overnight-stall class D-07 elsewhere exists to prevent.

---

## Finding 2 — Warning: C2 auto-mode unlock is a hardcoded enum membership, not the "explicit dogfood flag" its own comment mandates

**File:** `crates/devflow-cli/src/preflight.rs:978-983`

**Quoted snippet:**
```rust
/// ... Antigravity joins by replacing `state.agent == AgentKind::Claude`
/// with an explicit dogfood flag.
fn unattended_launch_shape_condition(state: &State) -> ConditionState {
    if stream_launch_enabled(state.agent, Stage::Code, state.legacy_claude_launch)
        && (state.agent == AgentKind::Claude || state.agent == AgentKind::Antigravity)
    {
        return ConditionState::Holds;
    }
```

The doc comment (and 42-02's own threat register, T-42-08 *"Premature auto-mode unlock — high"*) specifies the unlock be gated behind an **explicit dogfood flag**. What shipped is `|| state.agent == AgentKind::Antigravity` — a compile-time constant. There is no persisted "dogfooded" marker, no runtime evidence check. The comment's own earlier warning is now stale: it says *"Widening `stream_launch_enabled` to Antigravity (Task 3) would otherwise flip this condition to `Holds` and silently permit `--mode auto` for an UNDOGFOODED driver."* That widening already happened in Phase 41; Phase 42 just removed the last backstop by hand. The "proof" that the driver was dogfooded is a human's sentence in a markdown file (see Finding 3), which the code cannot and does not consult.

**Failure mode:** the unlock is permanent and unconditional. Any regression in Antigravity's stream handling, or a future re-classification of an undogfooded agent, silently ships unattended without any code-level gate re-asserting the dogfood precondition.

---

## Finding 3 — Warning: The dogfood "cadence measurement" contains no measurements; the evidence for removing T-42-08 is unverifiable

**File:** `.planning/phases/42-hermes-driver/42-VERIFICATION.md` (§ "Dogfood Cadence & Quiet-Gap Measurement") and `42-UAT.md` (Test 5)

**Quoted snippet:**
```
- **Observed Cadence**:
  - Stream events emitted regularly during tool dispatches, file reads, and shell executions.
  - Quiet gaps between events remained within bounds; no false-alarm idle timeout was observed.
```

42-02 Task 2's acceptance criterion was *"Quiet-gap event cadence distribution is **measured and compared against the 120s idle timeout floor**"*. The artifact delivers qualitative prose: no timestamps, no gap distribution, no max-gap value, no comparison to the 120s floor, no `--print-timeout 60m` log excerpt. This is the sole evidence basis for the Finding-2 unlock (ANTG-04/D-07), so a security-relevant refusal was removed on the strength of an assertion that cannot be reconstructed or falsified.

**Failure mode:** the "120s floor is sufficient" claim is asserted, not demonstrated; if the real floor is marginal under load, the auto-mode unlock ships on vibes.

---

## Finding 4 — Warning: `parse_hermes_tools_list_for_delegation` is a substring match against a fabricated output format, with a concrete false-positive

**File:** `crates/devflow-core/src/agents/hermes.rs:97-103`

**Quoted snippet:**
```rust
pub fn parse_hermes_tools_list_for_delegation(stdout: &str) -> bool {
    for line in stdout.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("delegation") && lower.contains("enabled") {
            return true;
        }
    }
    false
}
```

Two defects:

1. **False positive:** `contains("enabled")` matches the substring inside *"not enabled"* / *"enabled=false"*. A line reading `✗ delegation (not enabled)` — a perfectly plausible disabled-toolset rendering — returns `true`. The delegation probe would then advertise `subagent_dispatch: true` while delegation is actually off.
2. **Unverified format.** The only "specification" of the real output is the unit test's hand-invented fixture (`hermes.rs:159-167`, `✓ enabled delegation 👥 Task Delegation`). Nothing in `42-RESEARCH.md`/`42-CONTEXT.md` records what `hermes tools list` (v0.20.4) actually prints — whether `enabled`/`delegation` land on one line, two lines, or in a table. A real CLI emitting `delegation` on one line and `enabled` on the next would permanently fail closed (silently `false`).

The failure direction is mostly fail-closed (benign), but the *"not enabled"* case flips it fail-open on the capability that the driver uses to decide dispatch topology.

---

## Finding 5 — Info: No positive control — no test proves a marker-emitting Hermes can complete a commit-gated stage

**File:** `crates/devflow-cli/tests/phase7_cli.rs:1812-1928`

All three integration tests are negative controls: marker-less (`:1812`), non-zero exit (`:1844`), hung (`:1877`). None runs a `hermes` stub that emits `DEVFLOW_RESULT` and then asserts the run **advances through a commit-gated stage** (or to completion). So the suite proves three ways Hermes fails to advance, but never proves the happy path works at all — that a marker on stdout, parsed by the legacy `parse_marker_lines` path, actually advances Plan/Code/Validate/Ship. The only "evidence" Hermes completes anything is the `hermes_driver_render_prompt` unit test checking the rendered text *contains* `DEVFLOW_RESULT`, which says nothing about whether the marker is consumed correctly end-to-end.

**Failure mode:** a future change that breaks marker extraction for the legacy path would be invisible to `cargo test ... hermes`; the phase's only end-to-end assertions are "it correctly did nothing."

---

## Finding 6 — Info: `doctor_includes_hermes` test name drift + near-vacuous assertion

**File:** `crates/devflow-cli/src/commands.rs:6840-6851`

The plan (`42-01` acceptance criteria) and `42-UAT.md` Test 3 both claim a test named `doctor_includes_hermes` and report `cargo test ... doctor_includes_hermes` = "1 passed". The actual test is `doctor_includes_hermes_check_in_the_seam` — it passes only because cargo filters by substring. More substantively, the assertion is nearly empty:

```rust
fn doctor_includes_hermes_check_in_the_seam() {
    let checks = doctor_checks();
    let hermes = checks.iter().find(|c| c.name == "hermes")
        .expect("doctor_checks() must contain the hermes entry");
    if hermes.status == "missing" {           // ← on any machine with hermes installed, this is skipped
        assert!(hermes.install_hint...contains("hermes"));
    }
}
```

On a machine where `hermes` is present the test asserts only that the list *contains an entry named `"hermes"`* — the `.expect` itself — and nothing about its probe shape, version arg, or hint. This mirrors the pre-existing antigravity seam (`:6818`), but the naming mismatch against the plan/UAT is new and is exactly the `cargo test <name>` substring-trap this repo's own `CLAUDE.md` warns about.

---

## Finding 7 — Info: The C2 refusal for Hermes specifically is untested

**File:** `crates/devflow-cli/src/preflight.rs:3678-3719`

```rust
fn unattended_launch_shape_condition_non_stream_agent_refused() {
    let state = State::new(PhaseId::new(42), AgentKind::Pi, Mode::Auto, ...);
    ...
}
```

The non-stream refusal test uses `AgentKind::Pi`. Hermes — the phase's entire subject — is never asserted to be refused in `--mode auto`. The code path is shared (any `!= Claude && != Antigravity` agent is refused), so it *happens* to work, but the phase shipped an auto-mode-relevant behavior (Hermes is supervise-only) with no test pinning it. A future `driver_for`/`stream_launch_enabled` change that accidentally admits Hermes would not be caught by the phase's own suite.

---

## Finding 8 — Info: `HERMES_ACCEPT_HOOKS=1` is exported to the entire monitor process tree and is redundant with `--accept-hooks`

**File:** `crates/devflow-core/src/agents/hermes.rs:52-53` + `crates/devflow-core/src/monitor.rs:467-470`

```rust
fn environment(&self) -> Vec<(String, String)> {
    vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]
}
```

The legacy arm applies `.envs(...)` to the `sh` monitor, and the code documents that *"Adapter-scoped env … rides the whole monitor chain: sh → agent → its git children"* (`monitor.rs:467-470`). So `HERMES_ACCEPT_HOOKS=1` is inherited not just by `hermes` but by every git invocation and subprocess the agent spawns. It is also redundant: the same launch already passes `--accept-hooks` (`hermes.rs:47`). The env-var form is strictly broader than the flag form (process-tree-wide vs. single-process) for no added benefit. Low practical impact today (the var is Hermes-namespaced), but it is environment pollution wider than the stated intent.

---

## Finding 9 — Info: `HermesDriver::health()` spawns `hermes --version` on every stage preflight, via a raw (non-hermetic) `Command`

**File:** `crates/devflow-core/src/agents/hermes.rs:56-66` (called from `crates/devflow-cli/src/preflight.rs:1289`)

```rust
fn health(&self, _state: &crate::state::State) -> Result<(), String> {
    let output = std::process::Command::new("hermes")
        .arg("--version")
        .output()
        ...
}
```

`run_preflight` runs `generic_preflight_checks(...).and_then(|()| driver.health(state))` before **every** stage launch, so each Hermes stage pays an extra `hermes --version` subprocess. It is a raw `std::process::Command` — not `hermetic_command` — so it inherits ambient `GIT_DIR`/`GIT_CONFIG_*` pollution (harmless for `--version`, but inconsistent with the repo's explicit hermetic-spawn discipline). It also conflates "binary absent" (already definitively caught by `ensure_agent_binary` at start) with "version probe failed", producing a preflight refusal whose message (`hermes --version failed`) is indistinguishable from a transient spawn error.

---

## Summary

| # | Severity | Location | Defect |
|---|----------|----------|--------|
| 1 | Warning (High) | `monitor.rs:489`, `phase7_cli.rs:1877` | Hung Hermes never auto-reaped; the "cleanup" test kills the process itself — HRMS-03 unmet |
| 2 | Warning | `preflight.rs:978-983` | C2 unlock hardcoded (`\|\| Antigravity`), no dogfood flag; doc comment contradicted and now stale |
| 3 | Warning | `42-VERIFICATION.md`, `42-UAT.md` | Cadence "measurement" is qualitative prose — no numbers behind the gate removal |
| 4 | Warning | `hermes.rs:97-103` | Substring parse false-positives on "not enabled"; format is invented, not the real CLI's |
| 5 | Info | `phase7_cli.rs:1812-1928` | No positive control proving a marker-bearing Hermes advances a commit-gated stage |
| 6 | Info | `commands.rs:6840` | `doctor_includes_hermes` name drift + vacuous assertion |
| 7 | Info | `preflight.rs:3678` | Hermes auto-mode refusal untested (test uses `Pi`) |
| 8 | Info | `hermes.rs:52`, `monitor.rs:467` | `HERMES_ACCEPT_HOOKS=1` leaks process-tree-wide, redundant with `--accept-hooks` |
| 9 | Info | `hermes.rs:56`, `preflight.rs:1289` | Per-stage `hermes --version` spawn via raw, non-hermetic `Command` |

No command-injection or memory-safety defect was found: the legacy `sh` monitor passes the prompt through `"$@"` as a literal argv element (never shell-reparsed), so prompt content cannot inject. The most serious findings are the unimplemented hung-process handling (Finding 1) and the evidence-free, hardcoded auto-mode unlock (Findings 2–3).
