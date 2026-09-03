## Adversarial Review — Phase 42 (Hermes Driver + Antigravity Graduation)

I read the diff directly (`10bdeea..HEAD`), ran the actual test suites rather than trusting the plan docs' claims, and cross-checked against a prior `codex` review already sitting in this phase directory (`review_codex.md`) — several of its findings independently reproduce what I found; I note where I confirm, refine, or dispute them.

**Verified, not asserted:** `cargo test -p devflow-core --lib hermes` → real `14 passed` (not a `--exact` false-green). `cargo test -p devflow --bin devflow doctor_includes_hermes` → real `1 passed`. `cargo test -p devflow --test phase7_cli hermes` → real `3 passed` (confirmed by non-zero "filtered out" count in each case). The plan's claimed counts are accurate.

### Warning

**1. Delegation-toolset parser is co-occurrence matching, not status parsing — currently unreachable, but a live footgun the moment it's wired up.**
`crates/devflow-core/src/agents/hermes.rs:97-104`:
```rust
let lower = line.to_ascii_lowercase();
if lower.contains("delegation") && lower.contains("enabled") {
    return true;
}
```
Any single line containing both substrings anywhere returns `true`, regardless of which token they attach to — e.g. `✗ disabled delegation (see: enabled tools above)` reports delegation as available when it's disabled. The only test fixtures ever used are hand-authored, matching this exact bug's blind spot (`enabled`/`disabled` always cleanly separated by line); there's no citation anywhere in `42-RESEARCH.md` of the *real* `hermes tools list` output format, so both the implementation and its tests were written against an assumed shape, never the real CLI.
**But:** I grepped every call site of `.capabilities()` in the crate — `HermesDriver::capabilities()` (and thus this parser) is called from **nowhere** in production code. Pi's sibling probe is wired into a `devflow doctor` check (`commands.rs:2473`); Hermes's is not. So today this bug can't misinform an operator — it's dead code exercised only by its own unit tests. Flagging as Warning rather than Critical for that reason, but it will bite silently the first time someone adds the `hermes doctor` capability check the plan's own `42-CONTEXT.md` gestures at mirroring from Pi.

**2. The "hung process" integration test proves DevFlow does *not* recover from a hang — it requires a human to intervene.**
`crates/devflow-cli/tests/phase7_cli.rs:1900-1930` (`hermes_hung_process_is_detected_not_left_running`): the test spawns a hung `hermes`, then itself runs `kill <pid>` before the state is allowed to gate. That's the only thing that unblocks it. I checked `monitor.rs`'s idle-timeout machinery (`idle_timeout_setting_for`, `fire_idle_timeout`) — it operates on a line-by-line stdout channel (`line_rx.recv_timeout`), which is the *streaming* transport used by Claude/Antigravity only. Hermes uses the plain process-exit transport (blocking `.wait()`/`.output()`-style capture), which this codebase has never given a hang-detection mechanism (Codex/Pi/OpenCode have the identical gap). The test's own design is the proof: if the monitor could detect and kill a hang on its own, the manual `kill` step would be redundant. A genuinely hung `hermes` process today hangs the phase run indefinitely, discoverable only by a human watching. This is pre-existing to Hermes, not introduced by it, but it's precisely what HRMS-03 claims to verify and does not.

**3. `--legacy-claude-launch` silently no-ops for Antigravity, and `unattended_launch_shape_condition`'s cause-reporting is inconsistent about it.**
`crates/devflow-cli/src/pipeline_launch.rs:753-757`:
```rust
pub(crate) fn stream_launch_enabled(agent: AgentKind, stage: Stage, legacy_opt_out: bool) -> bool {
    matches!(agent, AgentKind::Claude | AgentKind::Antigravity)
        && STREAM_JSON_STAGES.contains(&stage)
        && !(agent == AgentKind::Claude && legacy_opt_out)
}
```
The opt-out only ever excludes Claude. So `State { agent: Antigravity, legacy_claude_launch: true, mode: Auto }` still gets `ConditionState::Holds` from `preflight.rs:981-985` — the same combination codex's review flagged. `--legacy-claude-launch` is CLI-accepted independent of `--agent` (`pipeline_launch.rs:1232` doc comment confirms it's "accepted here too"), so this combination is reachable from the command line, not just synthesizable in a test. Whether this is a genuine bug is arguable — the flag's own doc comment (`state.rs:360-380`) scopes it explicitly to "the pre-31 single-document **Claude** launch," so arguably Antigravity never had a legacy mode to opt out of. But the `causes` message-building code a few lines below (`preflight.rs:988-989`) still unconditionally checks `state.legacy_claude_launch` as if it were a live disqualifier for *any* agent — that check is now provably dead for Antigravity, and nothing documents that it's dead on purpose.

### Info

**4. Antigravity's unattended-mode graduation is a pure code predicate with no persisted link to the dogfood evidence that justified it.**
`preflight.rs:981-985`, tested by `unattended_launch_shape_condition_antigravity_allowed`. The graduation is permanent and unconditional from this commit forward — any future `State::new(_, Antigravity, Mode::Auto, _)` gets `Holds` regardless of whether the Antigravity binary in use has ever run supervised. There's no state field, config flag, or version pin tying the unlock to the specific dogfooded build described in `42-VERIFICATION.md`/`42-UAT.md`. If Antigravity's behavior regresses in a later release, nothing in this mechanism would re-detect it — the same class of one-way, evidence-detached trust decision this file's own D-09 doc comment (major-bump gate) is otherwise careful to avoid.

**5. `hermes.rs`'s two subprocess probes have no timeout.**
`hermes.rs:58-61` (`health`) and `hermes.rs:76-79` (`hermes_subagent_dispatch_available`) both call `.output()` with no timeout wrapper — an executable named `hermes` on `PATH` that hangs (bad shim, network-mounted binary stalling, etc.) blocks forever. I checked: this is the same idiom as every other `cmd_check`-based doctor probe in `commands.rs` (`Command::new(cmd).arg(version_arg).output()`, no timeout) — so it's systemic to this codebase, not a Hermes-specific regression. Still worth noting since it's new code that had the opportunity to fix it and didn't, and doctor probes for a hung binary would wedge `devflow doctor` entirely.

**6. Hermes, like Pi, doesn't override `interactivity_mode` for `Define` despite being documented as strictly oneshot.**
`hermes.rs`'s module doc: "Launches `hermes -z "<prompt>" --yolo --accept-hooks` in headless-safe **oneshot** mode." `CodexDriver` (`codex.rs:91`) exists specifically because a oneshot `exec`-style CLI can't drive the interactive multi-turn `discuss-phase` session, so it declares `RequiresExistingArtifact` for `Define`/`Plan`. `HermesDriver` (and `PiDriver`, which is equally oneshot per its own module doc) both fall through to the trait default `HeadlessSafe` for every stage, including `Define`. This mirrors Pi's already-shipped precedent exactly, so it's not a new defect this phase introduced — but it means the same architectural question Codex's override was built to answer was never re-asked for Hermes. I can't confirm from static reading alone whether a single `-z` turn is actually sufficient for Hermes to conduct `discuss-phase`; that's an untested assumption inherited wholesale from Pi.

**7. The shared conformance suite validates almost nothing driver-specific.**
`agents/mod.rs:146-169` (`contract_checks`) only asserts a non-empty name, `DEVFLOW_RESULT` in the rendered prompt per stage, and a non-empty program string. A `HermesDriver` returning `("hermes", vec!["--interactive"])` with no `-z`/`--yolo`/`--accept-hooks` and no environment would still pass `test_contract()` and the `hermes_conformance_enrollment` test. Pre-existing framework limitation shared by every driver, surfaced again here since `mod.rs`'s Hermes-specific test (`hermes_conformance_enrollment`) advertises "passes all 7 contract checks" as if that were meaningful proof of the launch shape — it isn't; the actual argv/env shape is only checked by `hermes_driver_build_command`/`hermes_driver_environment`, which are real but separate unit tests, not part of the enrolled suite.

---

No Critical-severity finding: argument passing goes through `std::process::Command`'s argv array (never a shell), so there's no injection vector in the `-z <prompt>` construction; `HERMES_ACCEPT_HOOKS=1` is scoped via `.envs()` to the child process only (verified against `monitor.rs`, not `std::env::set_var`); and the `--yolo`/`--accept-hooks` trust model matches the equally-permissive precedent already shipped for Claude/Codex/Antigravity.
