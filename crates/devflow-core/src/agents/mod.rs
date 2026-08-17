//! Agent adapter trait and implementations.
//!
//! Each adapter knows how to render a stage prompt for its agent and wrap it
//! into the CLI's non-interactive launch command. Prompt RENDERING is
//! driver-owned ([`AgentAdapter::render_prompt`]): Claude/OpenCode render the
//! legacy slash-command text, Codex renders a Codex-native instruction.

use crate::phase_id::PhaseId;
use crate::state::AgentKind;
use std::path::PathBuf;

/// Common behavior implemented by every supported coding-agent backend.
pub trait AgentAdapter {
    /// Human-readable adapter name.
    fn name(&self) -> &'static str;

    /// Build the command and arguments to launch this agent headless with the
    /// given `prompt` for `phase`. Returns `(program, args)`.
    ///
    /// `extra_writable_roots` are directories OUTSIDE the agent's working
    /// directory that its sandbox must still be allowed to write. Linked git
    /// worktrees keep their git metadata under the main repo's `.git/` — and
    /// Codex additionally read-only-mounts the cwd's resolved git dir, so
    /// BOTH the common `.git` and the worktree admin dir
    /// (`.git/worktrees/<name>`) must be granted explicitly (13-06 dogfood
    /// finding, verified with `codex sandbox` probes). Adapters without a
    /// sandbox ignore it.
    fn exec_command(
        &self,
        phase: PhaseId,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>);

    /// Extra environment variables for the agent process tree. Codex uses
    /// this to disable commit/tag signing inside its sandbox: the operator's
    /// signing agent (ssh-agent/gpg-agent) is unreachable there, so signed
    /// commits fail headless with a passphrase error (13-06 dogfood finding
    /// — same rationale as the unsigned VersionBump tags). `GIT_CONFIG_*`
    /// env scoping keeps the override out of every repo/global config.
    fn extra_env(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Detect an agent-specific completion signal in captured output.
    fn completion_signal_detected(&self, output: &str) -> bool;

    /// Adapter-specific pre-launch readiness check (D-13/D-14 adapter hook,
    /// Phase 17c). The default is a no-op — most adapters have nothing extra
    /// to check, mirroring [`Self::extra_env`]'s empty-default shape. The
    /// `Err` variant is a human-readable failure reason that flows into the
    /// preflight gate's context (`run_preflight` in `devflow-cli/src/main.rs`).
    /// This is the trait surface Phase 18's Hermes adapter implements to
    /// enforce a non-empty reviewer/receiver set — no built-in adapter
    /// (Claude/Codex/OpenCode) overrides it in Phase 17 because no
    /// reviewer-set storage exists yet in `state.rs`/`config.rs` (review
    /// consensus #6).
    fn preflight(&self, _state: &crate::state::State) -> Result<(), String> {
        Ok(())
    }

    /// Render the stage prompt for this agent from a [`crate::prompt::StageIntent`].
    ///
    /// This is the de-Claude-ification seam (999.31 / 37-01): the intent carries
    /// no agent syntax; each adapter turns it into its own instruction.
    /// Claude and OpenCode render the legacy slash-command text byte-for-byte
    /// (`crate::prompt::render_claude_style`); Codex renders a Codex-native
    /// instruction with no `/gsd-*` string.
    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String;
}

/// Capabilities a driver declares, enumerated as-needed (999.31 D-01).
/// `#[non_exhaustive]` + `Default` so adding a field never breaks an existing
/// driver (CONTEXT D-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct DriverCapabilities {}

/// What a driver's sandbox needs from the launch environment. Reserved for
/// 37-03 (Codex's writable-roots requirement).
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SandboxRequirements {}

/// One case from a driver's conformance contract (37-04).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractResult {
    pub name: &'static str,
    pub passed: bool,
}

/// Per-stage interactivity requirement a driver declares (999.31 / 31c),
/// replacing the hardcoded Codex-Define check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractivityMode {
    /// The stage can run headless with no pre-existing artifact or operator.
    HeadlessSafe,
    /// The stage needs a pre-existing artifact (e.g. Codex's Define needs a
    /// CONTEXT.md written ahead of time — it cannot run the interactive
    /// discuss-phase interview headless).
    RequiresExistingArtifact,
    /// The stage needs typed-subagent dispatch (e.g. `multi_agent_v2`).
    RequiresTypedSubagents,
    /// The stage cannot run headless at all.
    InteractiveOnly,
}

/// A driver's health classification, distinguishing "installed" from
/// "headless-usable" (999.31 / 31c).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverHealth {
    /// The binary is absent — `ensure_agent_binary` fails before health runs.
    BinaryAbsent,
    /// Installed but not headless-usable (e.g. no provider credential).
    NotHeadlessCapable(String),
    /// Ready to run headless.
    HeadlessCapable,
}

/// The modular driver contract (999.31): each agent owns its prompt rendering,
/// command building, completion parsing, and health/capability discovery —
/// instead of that logic being scattered across `prompt.rs`, `agents/*.rs`,
/// `agent_result.rs`, and `preflight.rs`.
pub trait AgentDriver {
    /// Human-readable driver name.
    fn name(&self) -> &'static str;

    /// Capabilities this driver declares (as-needed; default empty).
    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities::default()
    }

    /// Render the stage prompt for this agent from a [`crate::prompt::StageIntent`].
    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String;

    /// Build the command and arguments to launch this agent headless.
    fn build_command(
        &self,
        phase: PhaseId,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>);

    /// Parse this agent's completion signal out of captured output; `None` when
    /// the transport is process-exit (no event stream to scan).
    fn parse_completion(&self, _output: &str) -> Option<crate::agent_result::AgentResult> {
        None
    }

    /// Driver-specific pre-launch health check.
    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
        Ok(())
    }

    /// Extra environment variables for the agent process tree.
    fn environment(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Sandbox requirements for this agent's launch.
    fn sandbox_requirements(&self) -> SandboxRequirements {
        SandboxRequirements::default()
    }

    /// Discover capabilities from the installed CLI (e.g. `codex features list`).
    fn discover(&self) -> Result<(), String> {
        Ok(())
    }

    /// The conformance suite every driver must pass (37-04).
    fn test_contract(&self) -> Vec<ContractResult> {
        contract_checks(self)
    }

    /// The interactivity requirement for running `stage` headless.
    fn interactivity_mode(&self, _stage: crate::stage::Stage) -> InteractivityMode {
        InteractivityMode::HeadlessSafe
    }

    /// The directory holding this agent's GSD workflow files, used by the
    /// workflow-reference renderer. Defaults to the Codex install; a driver
    /// with a different install (e.g. Pi) overrides it.
    fn workflow_root(&self) -> String {
        "$HOME/.codex/gsd-core/workflows".to_string()
    }

    /// Classify this driver's health (the pass/fail [`AgentDriver::health`]
    /// mapped onto the richer [`DriverHealth`]).
    fn health_classification(&self, state: &crate::state::State) -> DriverHealth {
        match self.health(state) {
            Ok(()) => DriverHealth::HeadlessCapable,
            Err(reason) => DriverHealth::NotHeadlessCapable(reason),
        }
    }
}

/// Shared conformance checks every driver's `test_contract` runs (37-04).
/// A future driver (Antigravity, Hermes) plugs in by passing these — the
/// extensibility proof CONTEXT D-02 asks for.
fn contract_checks<D: AgentDriver + ?Sized>(driver: &D) -> Vec<ContractResult> {
    let mut checks = vec![ContractResult {
        name: "name is non-empty",
        passed: !driver.name().is_empty(),
    }];
    for stage in [
        crate::stage::Stage::Define,
        crate::stage::Stage::Plan,
        crate::stage::Stage::Code,
        crate::stage::Stage::Validate,
        crate::stage::Stage::Ship,
    ] {
        let intent = crate::prompt::StageIntent::for_stage(stage, PhaseId::new(1));
        let prompt = driver.render_prompt(&intent);
        checks.push(ContractResult {
            name: "render_prompt states the completion contract",
            passed: prompt.contains("DEVFLOW_RESULT"),
        });
    }
    let (program, _args) = driver.build_command(PhaseId::new(1), "contract", &[]);
    checks.push(ContractResult {
        name: "build_command names a program",
        passed: !program.is_empty(),
    });
    checks
}

/// Compatibility shim (D-11 removal point): exposes an [`AgentDriver`] through
/// the legacy [`AgentAdapter`] surface so every caller keeps compiling until
/// 37-04 migrates them and removes `AgentAdapter`.
struct DriverShim<D: AgentDriver>(D);

impl<D: AgentDriver> AgentAdapter for DriverShim<D> {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn exec_command(
        &self,
        phase: PhaseId,
        prompt: &str,
        extra_writable_roots: &[PathBuf],
    ) -> (&'static str, Vec<String>) {
        self.0.build_command(phase, prompt, extra_writable_roots)
    }
    fn extra_env(&self) -> Vec<(String, String)> {
        self.0.environment()
    }
    fn completion_signal_detected(&self, _output: &str) -> bool {
        // Every current driver is process-exit transport (the monitor detects
        // exit via kill -0); none emits an in-stream completion signal. This
        // matches the legacy adapters' behavior byte-for-byte.
        false
    }
    fn preflight(&self, state: &crate::state::State) -> Result<(), String> {
        self.0.health(state)
    }
    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
        self.0.render_prompt(intent)
    }
}

/// Return an adapter for a configured agent kind.
pub fn adapter_for(kind: AgentKind) -> Box<dyn AgentAdapter> {
    match kind {
        AgentKind::Claude => Box::new(DriverShim(ClaudeDriver)),
        AgentKind::Codex => Box::new(DriverShim(CodexDriver)),
        AgentKind::OpenCode => Box::new(DriverShim(OpenCodeDriver)),
        AgentKind::Pi => Box::new(DriverShim(PiDriver)),
    }
}

pub mod claude;
pub mod codex;
pub mod opencode;
pub mod pi;

pub use claude::{ClaudeAgent, ClaudeDriver};
pub use codex::{CodexAgent, CodexDriver};
pub use opencode::{OpenCodeAgent, OpenCodeDriver};
pub use pi::{PiAgent, PiDriver};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::stage_prompt;
    use crate::stage::Stage;

    #[test]
    fn adapter_for_returns_correct_names() {
        assert_eq!(adapter_for(AgentKind::Claude).name(), "Claude Code");
        assert_eq!(adapter_for(AgentKind::Codex).name(), "OpenAI Codex");
        assert_eq!(adapter_for(AgentKind::OpenCode).name(), "OpenCode");
        assert_eq!(adapter_for(AgentKind::Pi).name(), "Pi");
    }

    /// 37-02: the drivers reproduce the legacy adapter byte-for-byte (the shim
    /// delegates to them, so this guards against future drift when the
    /// `AgentAdapter` face is removed in 37-04).
    #[test]
    fn drivers_reproduce_legacy_adapter_behavior() {
        let intent = crate::prompt::StageIntent::for_stage(Stage::Code, PhaseId::new(7));

        // Claude: stream-json argv + byte-identical legacy prompt.
        let (program, args) = ClaudeDriver.build_command(PhaseId::new(7), "x", &[]);
        assert_eq!(program, "claude");
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--input-format" && w[1] == "stream-json")
        );
        assert_eq!(
            ClaudeDriver.render_prompt(&intent),
            crate::prompt::render_claude_style(&intent)
        );

        // OpenCode: positional `run <prompt>` + byte-identical legacy prompt.
        let (program, args) = OpenCodeDriver.build_command(PhaseId::new(7), "x", &[]);
        assert_eq!(program, "opencode");
        assert_eq!(args, ["run", "x"]);
        assert_eq!(
            OpenCodeDriver.render_prompt(&intent),
            crate::prompt::render_claude_style(&intent)
        );
    }

    /// 37-03: Codex/Pi drivers. Codex carries the verified non-interactive
    /// approval flag BEFORE `exec`; Pi keeps the Phase-36 `-p --no-approve`
    /// argv and renders the de-Claude-ified workflow prompt.
    #[test]
    fn codex_and_pi_drivers_reproduce_legacy_behavior() {
        let intent = crate::prompt::StageIntent::for_stage(Stage::Code, PhaseId::new(7));

        let (program, args) = CodexDriver.build_command(PhaseId::new(7), "x", &[]);
        assert_eq!(program, "codex");
        assert_eq!(
            &args[0..2],
            ["-a", "never"],
            "the global approval flag must precede `exec` (verified form): {args:?}"
        );
        assert!(args.contains(&"exec".to_string()));
        assert!(
            CodexDriver
                .render_prompt(&intent)
                .contains("execute-phase.md")
        );
        assert!(
            !CodexDriver
                .render_prompt(&intent)
                .contains("/gsd-execute-phase")
        );

        let (program, args) = PiDriver.build_command(PhaseId::new(7), "x", &[]);
        assert_eq!(program, "pi");
        assert_eq!(args, ["-p", "--no-approve", "x"]);
        assert!(PiDriver.render_prompt(&intent).contains("execute-phase.md"));
        assert!(
            !PiDriver
                .render_prompt(&intent)
                .contains("/gsd-execute-phase")
        );
    }

    /// 37-04: every driver passes the shared conformance suite, and Codex
    /// declares the Define/Plan interactivity requirement that replaces the
    /// hardcoded Codex-Define check.
    #[test]
    fn every_driver_passes_the_conformance_suite() {
        let drivers: [Box<dyn AgentDriver>; 4] = [
            Box::new(ClaudeDriver),
            Box::new(CodexDriver),
            Box::new(OpenCodeDriver),
            Box::new(PiDriver),
        ];
        for driver in &drivers {
            let results = driver.test_contract();
            assert!(
                !results.is_empty(),
                "{} has no conformance cases",
                driver.name()
            );
            for result in &results {
                assert!(
                    result.passed,
                    "{} failed conformance case {:?}",
                    driver.name(),
                    result.name
                );
            }
        }
    }

    /// A deliberately-broken driver: empty render + empty program. The suite
    /// must FAIL it — the negative control proving `test_contract` isn't
    /// vacuous (code-review finding #7).
    struct BrokenDriver;

    impl AgentDriver for BrokenDriver {
        fn name(&self) -> &'static str {
            "broken"
        }
        fn render_prompt(&self, _intent: &crate::prompt::StageIntent) -> String {
            String::new()
        }
        fn build_command(
            &self,
            _phase: PhaseId,
            _prompt: &str,
            _roots: &[PathBuf],
        ) -> (&'static str, Vec<String>) {
            ("", Vec::new())
        }
    }

    #[test]
    fn conformance_suite_fails_a_broken_driver() {
        let results = BrokenDriver.test_contract();
        assert!(
            results.iter().any(|r| !r.passed),
            "the conformance suite must fail a broken driver (empty render, empty program)"
        );
    }

    /// The workflow renderer must preserve the per-stage contracts (code-review
    /// findings #1-5): Validate verdict, Ship review gate, Define no-op, Plan
    /// idempotency, and a per-driver workflow root.
    #[test]
    fn workflow_render_preserves_stage_contracts() {
        use crate::prompt::StageIntent;
        use crate::stage::Stage;

        let codex = CodexDriver;

        // Validate demands the verdict (finding #1).
        let validate =
            codex.render_prompt(&StageIntent::for_stage(Stage::Validate, PhaseId::new(7)));
        assert!(validate.contains("\"verdict\": \"pass\""));
        assert!(validate.contains("\"verdict\": \"gaps\""));

        // Ship keeps the review gate (finding #2).
        let ship = codex.render_prompt(&StageIntent::for_stage(Stage::Ship, PhaseId::new(7)));
        assert!(ship.contains("Critical"));
        assert!(ship.contains("review:"));

        // Define is the D-14 no-op (finding #3).
        let define = codex.render_prompt(&StageIntent::for_stage(Stage::Define, PhaseId::new(7)));
        assert!(define.contains("must NOT run") || define.contains("do NOT run"));
        assert!(!define.contains("discuss-phase.md"));

        // Plan keeps the idempotency guard (finding #3).
        let plan = codex.render_prompt(&StageIntent::for_stage(Stage::Plan, PhaseId::new(7)));
        assert!(plan.contains("already exists"));

        // Pi points at its own workflow root (finding #5).
        let pi_code = PiDriver.render_prompt(&StageIntent::for_stage(Stage::Code, PhaseId::new(7)));
        assert!(pi_code.contains("$HOME/.pi/agent/gsd-core/workflows"));
        assert!(!pi_code.contains("$HOME/.codex/gsd-core"));
    }

    #[test]
    fn codex_define_and_plan_require_an_existing_artifact() {
        assert_eq!(
            CodexDriver.interactivity_mode(crate::stage::Stage::Define),
            InteractivityMode::RequiresExistingArtifact
        );
        assert_eq!(
            CodexDriver.interactivity_mode(crate::stage::Stage::Plan),
            InteractivityMode::RequiresExistingArtifact
        );
        assert_eq!(
            CodexDriver.interactivity_mode(crate::stage::Stage::Code),
            InteractivityMode::HeadlessSafe
        );
        assert_eq!(
            ClaudeDriver.interactivity_mode(crate::stage::Stage::Define),
            InteractivityMode::HeadlessSafe
        );
    }

    /// The shared-prompt invariant is retired (999.31 / 37-01): Claude and
    /// OpenCode still render byte-identical legacy text, but Codex now renders
    /// a Codex-native instruction instead of the shared `/gsd-*` slash command.
    #[test]
    fn claude_and_opencode_stay_identical_but_codex_renders_native() {
        let intent = crate::prompt::StageIntent::for_stage(Stage::Code, PhaseId::new(7));
        let claude = adapter_for(AgentKind::Claude).render_prompt(&intent);
        let opencode = adapter_for(AgentKind::OpenCode).render_prompt(&intent);
        let codex = adapter_for(AgentKind::Codex).render_prompt(&intent);

        // Claude/OpenCode: byte-identical legacy text (zero regression).
        assert_eq!(
            claude, opencode,
            "Claude and OpenCode must stay byte-identical after the migration"
        );
        assert_eq!(
            claude,
            stage_prompt(Stage::Code, PhaseId::new(7)),
            "Claude must render the legacy stage_prompt text byte-for-byte (CONTEXT D-01)"
        );

        // Codex: native, NOT the shared slash-command text (the dogfood fix).
        assert_ne!(
            codex, claude,
            "Codex must no longer render the shared slash-command text"
        );
        // Negative control, precise: no GSD slash COMMAND may appear (the
        // `gsd-core` workflow-directory path is legitimate and must not trip
        // a naive `/gsd-` substring check).
        for command in [
            "/gsd-discuss-phase",
            "/gsd-plan-phase",
            "/gsd-execute-phase",
            "/gsd-validate-phase",
            "/gsd-ship",
            "/gsd-code-review",
            "/gsd-audit-fix",
        ] {
            assert!(
                !codex.contains(command),
                "Codex render must not carry {command}: {codex}"
            );
        }
        // Positive oracle: the native instruction references the workflow path,
        // carries the --auto token, and states the completion contract (so an
        // empty or \"do nothing\" string cannot pass).
        assert!(codex.contains("execute-phase.md"));
        assert!(codex.contains("--auto"));
        assert!(codex.contains("DEVFLOW_RESULT"));
    }

    /// The Phase 31 launch contract, asserted as one thing because getting
    /// only the flags right is the documented way to half-implement it: the
    /// transport is `stream-json` in BOTH directions, and the prompt is not a
    /// positional argument at all.
    #[test]
    fn claude_launches_headless_stream_json_without_positional_prompt() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(3));
        let (program, args) =
            adapter_for(AgentKind::Claude).exec_command(PhaseId::new(3), &prompt, &[]);
        assert_eq!(program, "claude");
        assert!(args.iter().any(|a| a == "-p"));
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--input-format" && w[1] == "stream-json"),
            "the INPUT format is what moves the initial turn onto stdin; \
             flipping only the output format leaves the CLI with no first \
             turn and it stalls headless: {args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w[0] == "--output-format" && w[1] == "stream-json"),
            "the OUTPUT format is what makes the capture a JSONL event stream \
             the Layer 1 stream parser can read: {args:?}"
        );
        assert!(args.iter().any(|a| a == "--dangerously-skip-permissions"));
        assert!(
            !args.iter().any(|arg| arg.contains("DEVFLOW_RESULT")),
            "no positional prompt: the initial user turn travels on stdin, \
             written by the monitor: {args:?}"
        );
    }

    #[test]
    fn codex_wraps_prompt_in_exec_and_json() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
        let (program, args) =
            adapter_for(AgentKind::Codex).exec_command(PhaseId::new(7), &prompt, &[]);
        assert_eq!(program, "codex");
        let joined = args.join(" ");
        assert!(joined.contains("exec"));
        assert!(joined.contains("--sandbox workspace-write"));
        assert!(joined.contains("--json"));
    }

    #[test]
    fn opencode_wraps_prompt_in_run() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
        let (program, args) =
            adapter_for(AgentKind::OpenCode).exec_command(PhaseId::new(7), &prompt, &[]);
        assert_eq!(program, "opencode");
        assert_eq!(args, ["run", prompt.as_str()]);
    }

    /// 13-06 dogfood regression (Codex leg): linked-worktree git metadata
    /// lives under the main repo's `.git/` — outside the workspace-write
    /// sandbox — and Codex read-only-mounts the cwd's resolved git dir, so
    /// BOTH the common `.git` and the worktree admin dir must be granted
    /// (verified with `codex sandbox` probes). Without roots, no override.
    #[test]
    fn codex_grants_writable_roots_for_worktree_git_metadata() {
        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
        let roots = vec![
            PathBuf::from("/repo/.git"),
            PathBuf::from("/repo/.git/worktrees/phase-07"),
        ];
        let (_, args) =
            adapter_for(AgentKind::Codex).exec_command(PhaseId::new(7), &prompt, &roots);
        let joined = args.join(" ");
        assert!(
            joined.contains(
                r#"-c sandbox_workspace_write.writable_roots=["/repo/.git","/repo/.git/worktrees/phase-07"]"#
            ),
            "codex must whitelist the common .git AND the worktree admin dir: {joined}"
        );

        let (_, args) = adapter_for(AgentKind::Codex).exec_command(PhaseId::new(7), &prompt, &[]);
        assert!(
            !args.join(" ").contains("writable_roots"),
            "no override without an extra root"
        );
    }

    /// 13-06 dogfood regression: signed commits fail inside the Codex
    /// sandbox (no route to the operator's signing agent) — codex scopes an
    /// unsigned-commit override to its own process tree via GIT_CONFIG_*
    /// env; agents without a sandbox get no extra env.
    #[test]
    fn codex_disables_signing_via_env_others_do_not() {
        let env = adapter_for(AgentKind::Codex).extra_env();
        assert!(env.contains(&("GIT_CONFIG_KEY_0".into(), "commit.gpgsign".into())));
        assert!(env.contains(&("GIT_CONFIG_KEY_1".into(), "tag.gpgsign".into())));
        assert!(adapter_for(AgentKind::Claude).extra_env().is_empty());
        assert!(adapter_for(AgentKind::OpenCode).extra_env().is_empty());
    }

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
        assert!(adapter_for(AgentKind::Claude).preflight(&state).is_ok());
        assert!(adapter_for(AgentKind::Codex).preflight(&state).is_ok());
        assert!(adapter_for(AgentKind::OpenCode).preflight(&state).is_ok());
    }
}
