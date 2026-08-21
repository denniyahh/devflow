OpenAI Codex v0.147.0
--------
workdir: /var/home/denniyahh/Github/devflow/.worktrees/phase-42
model: gpt-5.6-luna
provider: openai
approval: on-request
sandbox: workspace-write [workdir, /tmp, $TMPDIR]
reasoning effort: medium
reasoning summaries: none
session id: 01a02645-2aa3-7bd2-89b0-342b4f941c6d
--------
user
You are an adversarial CODE reviewer. Attack the implementation of Phase 42 (Hermes Agent Driver & Antigravity Unattended Mode Graduation) in this repository. Do not summarize or praise the code. Your job is to find real, concrete defects with the exact file and line/quoted text for each finding.

Read and inspect the changed files directly from disk:
- crates/devflow-core/src/agents/hermes.rs
- crates/devflow-core/src/agents/mod.rs
- crates/devflow-core/src/state.rs
- crates/devflow-cli/src/commands.rs
- crates/devflow-cli/src/preflight.rs
- crates/devflow-cli/tests/phase7_cli.rs
- .planning/phases/42-hermes-driver/42-01-PLAN.md
- .planning/phases/42-hermes-driver/42-02-PLAN.md
- .planning/phases/42-hermes-driver/42-VERIFICATION.md
- .planning/phases/42-hermes-driver/42-UAT.md

Phase Context & Objectives:
- Goal: Implement modular HermesDriver for Hermes Agent CLI (`hermes -z "<prompt>" --yolo --accept-hooks`), with `HERMES_ACCEPT_HOOKS=1`, dynamic `hermes tools list` delegation probe, presence-only health probe, registration of AgentKind::Hermes, 6-driver conformance suite enrollment, devflow doctor presence probe, transport integration tests with MonitorReapGuard, and unlocking unattended auto mode for Antigravity in preflight.rs (C2 gate) following supervised dogfooding.

Hunt specifically for:
1. Subprocess & argument construction bugs: wrong flags for hermes spawn (`-z`, `--yolo`, `--accept-hooks`), prompt parameter carrier, quote escaping, missing `--` separator if needed, environment inheritance.
2. Security & permissions: command injection, environment pollution (`HERMES_ACCEPT_HOOKS=1`), shell hook bypass implications.
3. Preflight & condition state correctness: does `unattended_launch_shape_condition` correctly permit Antigravity while continuing to refuse unsupported/non-stream agents and legacy opt-outs?
4. Dynamic capabilities detection: parsing `hermes tools list` output for delegation toolset, case sensitivity, failure modes if CLI is absent/errors.
5. Error handling, exit codes, signal killing, and monitor liveness: unhandled error envelopes, hung process cleanup, MonitorReapGuard leakage.
6. test_contract() and conformance suite assertions: are assertions meaningful with negative controls?
7. Concurrency / state machine transitions: state serialization/deserialization, gate resolution, stop_until behavior.

Be specific, rigorous, and hostile. For each finding provide:
1. Severity (Critical, Warning, or Info)
2. File and line number / exact quoted snippet
3. Defect explanation (why it is broken)
4. Concrete failure mode or reproduction scenario
warning: failed to parse hooks config /var/home/denniyahh/Github/devflow/.codex/hooks.json: unknown field `state`, expected `description` or `hooks` at line 2 column 9
hook: SessionStart
hook: SessionStart Completed
hook: UserPromptSubmit
hook: UserPromptSubmit Completed
codex
I’m using the repository’s code-review skill because this is an adversarial implementation review; I’ll inspect the specified files and verify each claimed defect against the on-disk code.
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "sed -n '1,240p' .codex/skills/code-review/SKILL.md" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 278ms:
---
name: code-review
description: "[OMX] Run a comprehensive code review"
---

# Code Review Skill

Conduct a thorough code review for quality, security, and maintainability with severity-rated feedback.

## When to Use

This skill activates when:
- User requests "review this code", "code review"
- Before merging a pull request
- After implementing a major feature
- User wants quality assessment

## GPT-5.5 Guidance Alignment

- Default to outcome-first progress and completion reporting: state the target result, evidence, validation status, and stop condition before adding process detail.
- Treat newer user task updates as local overrides for the active workflow branch while preserving earlier non-conflicting constraints.
- If correctness depends on additional inspection, retrieval, execution, or verification, keep using the relevant tools until the review is grounded; stop once enough evidence exists.
- Continue through clear, low-risk, reversible next steps automatically; ask only when the next step is materially branching, destructive, credentialed, external-production, or preference-dependent.

Delegates to the `code-reviewer` and `architect` agents in parallel for a two-lane review:

1. **Identify Changes**
   - Run `git diff` to find changed files
   - Determine scope of review (specific files or entire PR)

2. **Launch Parallel Review Lanes**
   - **`code-reviewer` lane** - owns spec compliance, security, code quality, performance, and maintainability findings
   - **`architect` lane** - owns the devil's-advocate / design-tradeoff perspective
   - Both lanes run in parallel on a clean context with explicit scope and artifacts, and produce distinct outputs before final synthesis
   - If either lane cannot be launched or does not return evidence, report `independent review unavailable`; do **not** substitute the current/authoring lane, and do **not** approve or mark the review merge-ready.

3. **Review Categories**
   - **Security** - Hardcoded secrets, injection risks, XSS, CSRF
   - **Code Quality** - Function size, complexity, nesting depth
   - **Performance** - Algorithm efficiency, N+1 queries, caching
   - **Best Practices** - Naming, documentation, error handling
   - **Maintainability** - Duplication, coupling, testability

4. **Severity Rating**
   - **CRITICAL** - Security vulnerability (must fix before merge)
   - **HIGH** - Bug or major code smell (should fix before merge)
   - **MEDIUM** - Minor issue (fix when possible)
   - **LOW** - Style/suggestion (consider fixing)

5. **Architectural Status Contract**
   - **CLEAR** - No unresolved architectural blocker was found
   - **WATCH** - Non-blocking design/tradeoff concern that must appear in the final synthesis
   - **BLOCK** - Unresolved design concern that prevents a merge-ready verdict

6. **Specific Recommendations**
   - File:line locations for each issue
   - Concrete fix suggestions
   - Code examples where applicable

7. **Final Synthesis**
   - Combine the `code-reviewer` recommendation and the architect status into one final verdict
   - Approval requires explicit evidence from both independent lanes; missing or failed delegation is a blocking unavailable-review state, not an approval fallback
   - Deterministic merge gating rules:
     - If architect status is **BLOCK**, final recommendation is **REQUEST CHANGES**
     - Else if `code-reviewer` recommendation is **REQUEST CHANGES**, final recommendation is **REQUEST CHANGES**
     - Else if architect status is **WATCH**, final recommendation is **COMMENT**
     - Else final recommendation follows the `code-reviewer` lane
   - The final report must make architect blockers impossible to miss

## Agent Delegation

Do not self-review as a fallback. If the `code-reviewer` or `architect` agent path is missing, unavailable, skipped, or fails, emit a clear unavailable-review result and block approval until the independent lane evidence exists.

Respect the user's current model and reasoning/effort selection when launching review lanes. Do not pass `model` or `reasoning_effort` overrides in the review-lane task calls unless the user explicitly asks for review-specific overrides; omitting them lets native subagents inherit the active session settings.

```
task(
  agent_type="code-reviewer",
  prompt="CODE REVIEW TASK

Review code changes for quality, security, and maintainability.

This is the code/spec/security lane. Do not absorb architectural ownership.

Scope: [git diff or specific files]

Review Checklist:
- Security vulnerabilities (OWASP Top 10)
- Code quality (complexity, duplication)
- Performance issues (N+1, inefficient algorithms)
- Best practices (naming, documentation, error handling)
- Maintainability (coupling, testability)

Output: Code review report with:
- Files reviewed count
- Issues by severity (CRITICAL, HIGH, MEDIUM, LOW)
- Specific file:line locations
- Fix recommendations
- Approval recommendation (APPROVE / REQUEST CHANGES / COMMENT)"
)

task(
  agent_type="architect",
  prompt="ARCHITECTURE / DEVIL'S-ADVOCATE REVIEW TASK

Review the same code changes from the architecture/tradeoff perspective.

Scope: [git diff or specific files]

Focus:
- System boundaries and interfaces
- Hidden coupling or long-term maintainability risks
- Tradeoff tension the main reviewer might miss
- Strongest counterargument against approving as-is

Output:
- Architectural Status: CLEAR / WATCH / BLOCK
- File:line evidence for each concern
- Concrete tradeoff or design recommendation"
)

Run both lanes in parallel, then synthesize them with the deterministic rules above.
```

## External Model Consultation (Preferred)

The code-reviewer agent SHOULD consult Codex for cross-validation.

### Protocol
1. **Form your OWN review FIRST** - Complete the review independently
2. **Consult for validation** - Cross-check findings with Codex
3. **Critically evaluate** - Never blindly adopt external findings
4. **Graceful optional consultation fallback** - Never block because optional external consultation tools are unavailable; this does not waive the required independent `code-reviewer` and `architect` lanes

### When to Consult
- Security-sensitive code changes
- Complex architectural patterns
- Unfamiliar codebases or languages
- High-stakes production code

### When to Skip
- Simple refactoring
- Well-understood patterns
- Time-critical reviews
- Small, isolated changes

### Tool Usage
Prefer native `code-reviewer` agent consultation or CLI-backed `ask_codex` surfaces when available. Optional MCP compatibility ask tools may be used only when already enabled. If optional external consultation tools are unavailable, continue with the required independent `code-reviewer` and `architect` lanes; do not replace those lanes with self-review.

**Note:** Codex calls can take up to 1 hour. Consider the review timeline before consulting.

## Output Format

```
CODE REVIEW REPORT
==================

Files Reviewed: 8
Total Issues: 12
Architectural Status: WATCH

CRITICAL (0)
-----------
(none)

HIGH (0)
--------
(none)

MEDIUM (7)
----------
1. src/api/auth.ts:42
   Issue: Email normalization logic is duplicated instead of reusing the shared helper
   Risk: Validation rules can drift between authentication paths
   Fix: Route both paths through the shared normalization helper

2. src/components/UserProfile.tsx:89
   Issue: Derived permissions are recalculated on every render
   Risk: Avoidable work during profile refreshes
   Fix: Memoize the derived permissions list or compute it upstream

3. src/utils/validation.ts:15
   Issue: Form-layer and server-layer validation messages are defined separately
   Risk: User-facing validation guidance can become inconsistent
   Fix: Share one validation message helper across both call sites

LOW (5)
-------
...

ARCHITECTURE WATCHLIST
----------------------
- src/review/orchestrator.ts:88
  Concern: Review result synthesis relies on implicit ordering rather than an explicit blocker contract
  Status: WATCH
  Recommendation: Define deterministic merge gating before expanding reviewers

SYNTHESIS
---------
- code-reviewer recommendation: COMMENT
- architect status: WATCH
- final recommendation: COMMENT

RECOMMENDATION: COMMENT

Address any WATCH concerns before treating the change as merge-ready.
```

## Review Checklist

The `code-reviewer` lane checks:

### Security
- [ ] No hardcoded secrets (API keys, passwords, tokens)
- [ ] All user inputs sanitized
- [ ] SQL/NoSQL injection prevention
- [ ] XSS prevention (escaped outputs)
- [ ] CSRF protection on state-changing operations
- [ ] Authentication/authorization properly enforced

### Code Quality
- [ ] Functions < 50 lines (guideline)
- [ ] Cyclomatic complexity < 10
- [ ] No deeply nested code (> 4 levels)
- [ ] No duplicate logic (DRY principle)
- [ ] Clear, descriptive naming

### Performance
- [ ] No N+1 query patterns
- [ ] Appropriate caching where applicable
- [ ] Efficient algorithms (avoid O(n²) when O(n) possible)
- [ ] No unnecessary re-renders (React/Vue)

### Best Practices
- [ ] Error handling present and appropriate
- [ ] Logging at appropriate levels
- [ ] Documentation for public APIs
- [ ] Tests for critical paths
- [ ] No commented-out code


hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
2026-08-21T21:41:27.961686Z ERROR codex_core::tools::router: error=Full-history forked agents inherit the parent agent type; omit agent_type, or spawn without a full-history fork.
hook: PreToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc 'git diff --stat && nl -ba crates/devflow-core/src/agents/hermes.rs && nl -ba crates/devflow-core/src/agents/mod.rs && nl -ba crates/devflow-core/src/state.rs' in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 122ms:
     1	//! Hermes coding-agent adapter (Phase 42).
     2	//!
     3	//! Launches `hermes -z "<prompt>" --yolo --accept-hooks` in headless-safe oneshot mode.
     4	//! The prompt is passed via `-z`. Environment variable `HERMES_ACCEPT_HOOKS=1` is injected
     5	//! to avoid interactive prompts on shell hooks.
     6	//!
     7	//! Slash commands (`/gsd-*`) are rendered via standard claude-style prompt rendering.
     8	//! Subagent dispatch capability is dynamically probed via `hermes tools list` checking for
     9	//! the enabled `delegation` toolset.
    10	
    11	use super::AgentDriver;
    12	use crate::phase_id::PhaseId;
    13	use std::path::PathBuf;
    14	
    15	/// The modular driver for Hermes (Phase 42): headless `-z` oneshot launch,
    16	/// `HERMES_ACCEPT_HOOKS=1` environment, standard claude-style prompt rendering,
    17	/// and dynamic delegation subagent dispatch probing.
    18	pub struct HermesDriver;
    19	
    20	impl AgentDriver for HermesDriver {
    21	    fn name(&self) -> &'static str {
    22	        "Hermes"
    23	    }
    24	
    25	    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String {
    26	        crate::prompt::render_claude_style(intent)
    27	    }
    28	
    29	    fn capabilities(&self) -> super::DriverCapabilities {
    30	        super::DriverCapabilities {
    31	            subagent_dispatch: hermes_subagent_dispatch_available(),
    32	        }
    33	    }
    34	
    35	    fn build_command(
    36	        &self,
    37	        _phase: PhaseId,
    38	        prompt: &str,
    39	        _extra_writable_roots: &[PathBuf],
    40	    ) -> (&'static str, Vec<String>) {
    41	        (
    42	            "hermes",
    43	            vec![
    44	                "-z".into(),
    45	                prompt.to_string(),
    46	                "--yolo".into(),
    47	                "--accept-hooks".into(),
    48	            ],
    49	        )
    50	    }
    51	
    52	    fn environment(&self) -> Vec<(String, String)> {
    53	        vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]
    54	    }
    55	
    56	    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
    57	        // Presence-only probe of the `hermes` binary.
    58	        let output = std::process::Command::new("hermes")
    59	            .arg("--version")
    60	            .output()
    61	            .map_err(|e| format!("could not run `hermes --version`: {e}"))?;
    62	        if output.status.success() {
    63	            Ok(())
    64	        } else {
    65	            let detail = String::from_utf8_lossy(&output.stderr);
    66	            Err(format!("`hermes --version` failed: {}", detail.trim()))
    67	        }
    68	    }
    69	}
    70	
    71	/// Dynamically probe whether Hermes has the `delegation` toolset enabled.
    72	///
    73	/// Runs `hermes tools list` and checks for both `enabled` and `delegation` in the output.
    74	pub fn hermes_subagent_dispatch_available() -> bool {
    75	    hermes_subagent_dispatch_available_with(|| {
    76	        std::process::Command::new("hermes")
    77	            .args(["tools", "list"])
    78	            .output()
    79	    })
    80	}
    81	
    82	/// Inner helper parameterized on output function for unit testing without invoking real CLI.
    83	pub fn hermes_subagent_dispatch_available_with(
    84	    output_fn: impl FnOnce() -> Result<std::process::Output, std::io::Error>,
    85	) -> bool {
    86	    let Ok(output) = output_fn() else {
    87	        return false;
    88	    };
    89	    if !output.status.success() {
    90	        return false;
    91	    }
    92	    let stdout = String::from_utf8_lossy(&output.stdout);
    93	    parse_hermes_tools_list_for_delegation(&stdout)
    94	}
    95	
    96	/// Parse `hermes tools list` stdout to check if delegation toolset is enabled.
    97	pub fn parse_hermes_tools_list_for_delegation(stdout: &str) -> bool {
    98	    for line in stdout.lines() {
    99	        let lower = line.to_ascii_lowercase();
   100	        if lower.contains("delegation") && lower.contains("enabled") {
   101	            return true;
   102	        }
   103	    }
   104	    false
   105	}
   106	
   107	#[cfg(test)]
   108	mod tests {
   109	    use super::*;
   110	    use crate::phase_id::PhaseId;
   111	    use crate::stage::Stage;
   112	    use std::os::unix::process::ExitStatusExt;
   113	
   114	    #[test]
   115	    fn hermes_driver_name() {
   116	        let driver = HermesDriver;
   117	        assert_eq!(driver.name(), "Hermes");
   118	    }
   119	
   120	    #[test]
   121	    fn hermes_driver_build_command() {
   122	        let driver = HermesDriver;
   123	        let (prog, args) = driver.build_command(PhaseId::new(42), "test prompt", &[]);
   124	        assert_eq!(prog, "hermes");
   125	        assert_eq!(
   126	            args,
   127	            vec![
   128	                "-z".to_string(),
   129	                "test prompt".to_string(),
   130	                "--yolo".to_string(),
   131	                "--accept-hooks".to_string(),
   132	            ]
   133	        );
   134	    }
   135	
   136	    #[test]
   137	    fn hermes_driver_environment() {
   138	        let driver = HermesDriver;
   139	        let envs = driver.environment();
   140	        assert_eq!(envs, vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]);
   141	    }
   142	
   143	    #[test]
   144	    fn hermes_driver_render_prompt() {
   145	        let driver = HermesDriver;
   146	        let intent = crate::prompt::StageIntent::for_stage(Stage::Plan, PhaseId::new(42));
   147	        let rendered = driver.render_prompt(&intent);
   148	        assert!(rendered.contains("DEVFLOW_RESULT"));
   149	        assert!(rendered.contains("/gsd-plan-phase 42"));
   150	    }
   151	
   152	    #[test]
   153	    fn parse_hermes_tools_list_delegation_enabled() {
   154	        let sample = "\
   155	Available Toolsets:
   156	  ✓ enabled delegation 👥 Task Delegation
   157	  ✓ enabled terminal   💻 Terminal Execution
   158	  ✗ disabled web       🌐 Web Search
   159	";
   160	        assert!(parse_hermes_tools_list_for_delegation(sample));
   161	    }
   162	
   163	    #[test]
   164	    fn parse_hermes_tools_list_delegation_disabled() {
   165	        let sample = "\
   166	Available Toolsets:
   167	  ✗ disabled delegation 👥 Task Delegation
   168	  ✓ enabled terminal   💻 Terminal Execution
   169	";
   170	        assert!(!parse_hermes_tools_list_for_delegation(sample));
   171	    }
   172	
   173	    #[test]
   174	    fn parse_hermes_tools_list_missing_delegation() {
   175	        let sample = "\
   176	Available Toolsets:
   177	  ✓ enabled terminal   💻 Terminal Execution
   178	";
   179	        assert!(!parse_hermes_tools_list_for_delegation(sample));
   180	    }
   181	
   182	    #[test]
   183	    fn hermes_subagent_dispatch_with_mock() {
   184	        let success_output = || {
   185	            Ok(std::process::Output {
   186	                status: std::process::ExitStatus::from_raw(0),
   187	                stdout: b"  \xe2\x9c\x93 enabled delegation \xf0\x9f\x91\xa5 Task Delegation\n".to_vec(),
   188	                stderr: Vec::new(),
   189	            })
   190	        };
   191	        assert!(hermes_subagent_dispatch_available_with(success_output));
   192	
   193	        let failure_output = || {
   194	            Ok(std::process::Output {
   195	                status: std::process::ExitStatus::from_raw(1 << 8),
   196	                stdout: b"error\n".to_vec(),
   197	                stderr: Vec::new(),
   198	            })
   199	        };
   200	        assert!(!hermes_subagent_dispatch_available_with(failure_output));
   201	
   202	        let io_error = || Err(std::io::Error::new(std::io::ErrorKind::NotFound, "not found"));
   203	        assert!(!hermes_subagent_dispatch_available_with(io_error));
   204	    }
   205	}
     1	//! Agent driver contract and implementations.
     2	//!
     3	//! Each driver knows how to render a stage prompt for its agent and wrap it
     4	//! into the CLI's non-interactive launch command. Prompt RENDERING is
     5	//! driver-owned ([`AgentDriver::render_prompt`]): Claude/OpenCode render the
     6	//! legacy slash-command text, Codex renders a Codex-native instruction.
     7	
     8	use crate::phase_id::PhaseId;
     9	use crate::state::AgentKind;
    10	use std::path::PathBuf;
    11	
    12	/// Capabilities a driver declares, enumerated as-needed (999.31 D-01).
    13	/// `#[non_exhaustive]` + `Default` so adding a field never breaks an existing
    14	/// driver (CONTEXT D-12).
    15	#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    16	#[non_exhaustive]
    17	pub struct DriverCapabilities {
    18	    /// Whether the agent has subagent/dispatch capability available in its
    19	    /// profile (e.g. Pi's `@bacnh85/pi-subagent` extension). Detected by probing
    20	    /// the installed CLI; `false` when absent or undetectable (fail-closed to
    21	    /// the baseline single-agent path).
    22	    pub subagent_dispatch: bool,
    23	}
    24	
    25	/// What a driver's sandbox needs from the launch environment. Reserved for
    26	/// 37-03 (Codex's writable-roots requirement).
    27	#[derive(Debug, Clone, Default)]
    28	#[non_exhaustive]
    29	pub struct SandboxRequirements {}
    30	
    31	/// One case from a driver's conformance contract (37-04).
    32	#[derive(Debug, Clone, PartialEq, Eq)]
    33	pub struct ContractResult {
    34	    pub name: &'static str,
    35	    pub passed: bool,
    36	}
    37	
    38	/// Per-stage interactivity requirement a driver declares (999.31 / 31c),
    39	/// replacing the hardcoded Codex-Define check.
    40	#[derive(Debug, Clone, Copy, PartialEq, Eq)]
    41	pub enum InteractivityMode {
    42	    /// The stage can run headless with no pre-existing artifact or operator.
    43	    HeadlessSafe,
    44	    /// The stage needs a pre-existing artifact (e.g. Codex's Define needs a
    45	    /// CONTEXT.md written ahead of time — it cannot run the interactive
    46	    /// discuss-phase interview headless).
    47	    RequiresExistingArtifact,
    48	    /// The stage needs typed-subagent dispatch (e.g. `multi_agent_v2`).
    49	    RequiresTypedSubagents,
    50	    /// The stage cannot run headless at all.
    51	    InteractiveOnly,
    52	}
    53	
    54	/// A driver's health classification, distinguishing "installed" from
    55	/// "headless-usable" (999.31 / 31c).
    56	#[derive(Debug, Clone, PartialEq, Eq)]
    57	pub enum DriverHealth {
    58	    /// The binary is absent — `ensure_agent_binary` fails before health runs.
    59	    BinaryAbsent,
    60	    /// Installed but not headless-usable (e.g. no provider credential).
    61	    NotHeadlessCapable(String),
    62	    /// Ready to run headless.
    63	    HeadlessCapable,
    64	}
    65	
    66	/// The modular driver contract (999.31): each agent owns its prompt rendering,
    67	/// command building, completion parsing, and health/capability discovery —
    68	/// instead of that logic being scattered across `prompt.rs`, `agents/*.rs`,
    69	/// `agent_result.rs`, and `preflight.rs`.
    70	pub trait AgentDriver {
    71	    /// Human-readable driver name.
    72	    fn name(&self) -> &'static str;
    73	
    74	    /// Capabilities this driver declares (as-needed; default empty).
    75	    fn capabilities(&self) -> DriverCapabilities {
    76	        DriverCapabilities::default()
    77	    }
    78	
    79	    /// Render the stage prompt for this agent from a [`crate::prompt::StageIntent`].
    80	    fn render_prompt(&self, intent: &crate::prompt::StageIntent) -> String;
    81	
    82	    /// Build the command and arguments to launch this agent headless.
    83	    fn build_command(
    84	        &self,
    85	        phase: PhaseId,
    86	        prompt: &str,
    87	        extra_writable_roots: &[PathBuf],
    88	    ) -> (&'static str, Vec<String>);
    89	
    90	    /// Parse this agent's completion signal out of captured output; `None` when
    91	    /// the transport is process-exit (no event stream to scan).
    92	    fn parse_completion(&self, _output: &str) -> Option<crate::agent_result::AgentResult> {
    93	        None
    94	    }
    95	
    96	    /// Driver-specific pre-launch health check.
    97	    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
    98	        Ok(())
    99	    }
   100	
   101	    /// Extra environment variables for the agent process tree.
   102	    fn environment(&self) -> Vec<(String, String)> {
   103	        Vec::new()
   104	    }
   105	
   106	    /// Sandbox requirements for this agent's launch.
   107	    fn sandbox_requirements(&self) -> SandboxRequirements {
   108	        SandboxRequirements::default()
   109	    }
   110	
   111	    /// Discover capabilities from the installed CLI (e.g. `codex features list`).
   112	    fn discover(&self) -> Result<(), String> {
   113	        Ok(())
   114	    }
   115	
   116	    /// The conformance suite every driver must pass (37-04).
   117	    fn test_contract(&self) -> Vec<ContractResult> {
   118	        contract_checks(self)
   119	    }
   120	
   121	    /// The interactivity requirement for running `stage` headless.
   122	    fn interactivity_mode(&self, _stage: crate::stage::Stage) -> InteractivityMode {
   123	        InteractivityMode::HeadlessSafe
   124	    }
   125	
   126	    /// The directory holding this agent's GSD workflow files, used by the
   127	    /// workflow-reference renderer. Defaults to the Codex install; a driver
   128	    /// with a different install (e.g. Pi) overrides it.
   129	    fn workflow_root(&self) -> String {
   130	        "$HOME/.codex/gsd-core/workflows".to_string()
   131	    }
   132	
   133	    /// Classify this driver's health (the pass/fail [`AgentDriver::health`]
   134	    /// mapped onto the richer [`DriverHealth`]).
   135	    fn health_classification(&self, state: &crate::state::State) -> DriverHealth {
   136	        match self.health(state) {
   137	            Ok(()) => DriverHealth::HeadlessCapable,
   138	            Err(reason) => DriverHealth::NotHeadlessCapable(reason),
   139	        }
   140	    }
   141	}
   142	
   143	/// Shared conformance checks every driver's `test_contract` runs (37-04).
   144	/// A future driver (Antigravity, Hermes) plugs in by passing these — the
   145	/// extensibility proof CONTEXT D-02 asks for.
   146	fn contract_checks<D: AgentDriver + ?Sized>(driver: &D) -> Vec<ContractResult> {
   147	    let mut checks = vec![ContractResult {
   148	        name: "name is non-empty",
   149	        passed: !driver.name().is_empty(),
   150	    }];
   151	    for stage in [
   152	        crate::stage::Stage::Define,
   153	        crate::stage::Stage::Plan,
   154	        crate::stage::Stage::Code,
   155	        crate::stage::Stage::Validate,
   156	        crate::stage::Stage::Ship,
   157	    ] {
   158	        let intent = crate::prompt::StageIntent::for_stage(stage, PhaseId::new(1));
   159	        let prompt = driver.render_prompt(&intent);
   160	        checks.push(ContractResult {
   161	            name: "render_prompt states the completion contract",
   162	            passed: prompt.contains("DEVFLOW_RESULT"),
   163	        });
   164	    }
   165	    let (program, _args) = driver.build_command(PhaseId::new(1), "contract", &[]);
   166	    checks.push(ContractResult {
   167	        name: "build_command names a program",
   168	        passed: !program.is_empty(),
   169	    });
   170	    checks
   171	}
   172	
   173	/// Return the driver for a configured agent kind.
   174	pub fn driver_for(kind: AgentKind) -> Box<dyn AgentDriver> {
   175	    match kind {
   176	        AgentKind::Claude => Box::new(ClaudeDriver),
   177	        AgentKind::Codex => Box::new(CodexDriver),
   178	        AgentKind::OpenCode => Box::new(OpenCodeDriver),
   179	        AgentKind::Pi => Box::new(PiDriver),
   180	        AgentKind::Antigravity => Box::new(AntigravityDriver),
   181	        AgentKind::Hermes => Box::new(HermesDriver),
   182	    }
   183	}
   184	
   185	pub mod antigravity;
   186	pub mod claude;
   187	pub mod codex;
   188	pub mod hermes;
   189	pub mod opencode;
   190	pub mod pi;
   191	
   192	pub use antigravity::AntigravityDriver;
   193	pub use claude::ClaudeDriver;
   194	pub use codex::CodexDriver;
   195	pub use hermes::HermesDriver;
   196	pub use opencode::OpenCodeDriver;
   197	pub use pi::PiDriver;
   198	
   199	#[cfg(test)]
   200	mod tests {
   201	    use super::*;
   202	    use crate::prompt::stage_prompt;
   203	    use crate::stage::Stage;
   204	
   205	    #[test]
   206	    fn driver_for_returns_correct_names() {
   207	        assert_eq!(driver_for(AgentKind::Claude).name(), "Claude Code");
   208	        assert_eq!(driver_for(AgentKind::Codex).name(), "OpenAI Codex");
   209	        assert_eq!(driver_for(AgentKind::OpenCode).name(), "OpenCode");
   210	        assert_eq!(driver_for(AgentKind::Pi).name(), "Pi");
   211	        assert_eq!(driver_for(AgentKind::Antigravity).name(), "Antigravity");
   212	        assert_eq!(driver_for(AgentKind::Hermes).name(), "Hermes");
   213	    }
   214	
   215	    /// 37-02: the drivers reproduce the legacy adapter byte-for-byte (the shim
   216	    /// delegated to them, so this guards against future drift now that the
   217	    /// legacy surface is removed).
   218	    #[test]
   219	    fn drivers_reproduce_legacy_adapter_behavior() {
   220	        let intent = crate::prompt::StageIntent::for_stage(Stage::Code, PhaseId::new(7));
   221	
   222	        // Claude: stream-json argv + byte-identical legacy prompt.
   223	        let (program, args) = ClaudeDriver.build_command(PhaseId::new(7), "x", &[]);
   224	        assert_eq!(program, "claude");
   225	        assert!(
   226	            args.windows(2)
   227	                .any(|w| w[0] == "--input-format" && w[1] == "stream-json")
   228	        );
   229	        assert_eq!(
   230	            ClaudeDriver.render_prompt(&intent),
   231	            crate::prompt::render_claude_style(&intent)
   232	        );
   233	
   234	        // OpenCode: positional `run <prompt>` + byte-identical legacy prompt.
   235	        let (program, args) = OpenCodeDriver.build_command(PhaseId::new(7), "x", &[]);
   236	        assert_eq!(program, "opencode");
   237	        assert_eq!(args, ["run", "x"]);
   238	        assert_eq!(
   239	            OpenCodeDriver.render_prompt(&intent),
   240	            crate::prompt::render_claude_style(&intent)
   241	        );
   242	    }
   243	
   244	    /// 37-03: Codex/Pi drivers. Codex carries the verified non-interactive
   245	    /// approval flag BEFORE `exec`; Pi keeps the Phase-36 `-p --no-approve`
   246	    /// argv and renders the de-Claude-ified workflow prompt.
   247	    #[test]
   248	    fn codex_and_pi_drivers_reproduce_legacy_behavior() {
   249	        let intent = crate::prompt::StageIntent::for_stage(Stage::Code, PhaseId::new(7));
   250	
   251	        let (program, args) = CodexDriver.build_command(PhaseId::new(7), "x", &[]);
   252	        assert_eq!(program, "codex");
   253	        assert_eq!(
   254	            &args[0..2],
   255	            ["-a", "never"],
   256	            "the global approval flag must precede `exec` (verified form): {args:?}"
   257	        );
   258	        assert!(args.contains(&"exec".to_string()));
   259	        assert!(
   260	            CodexDriver
   261	                .render_prompt(&intent)
   262	                .contains("execute-phase.md")
   263	        );
   264	        assert!(
   265	            !CodexDriver
   266	                .render_prompt(&intent)
   267	                .contains("/gsd-execute-phase")
   268	        );
   269	
   270	        let (program, args) = PiDriver.build_command(PhaseId::new(7), "x", &[]);
   271	        assert_eq!(program, "pi");
   272	        assert_eq!(args, ["-p", "--no-approve", "x"]);
   273	        assert!(PiDriver.render_prompt(&intent).contains("execute-phase.md"));
   274	        assert!(
   275	            !PiDriver
   276	                .render_prompt(&intent)
   277	                .contains("/gsd-execute-phase")
   278	        );
   279	    }
   280	
   281	    /// 37-04: every driver passes the shared conformance suite, and Codex
   282	    /// declares the Define/Plan interactivity requirement that replaces the
   283	    /// hardcoded Codex-Define check.
   284	    #[test]
   285	    fn every_driver_passes_the_conformance_suite() {
   286	        let drivers: [Box<dyn AgentDriver>; 6] = [
   287	            Box::new(ClaudeDriver),
   288	            Box::new(CodexDriver),
   289	            Box::new(OpenCodeDriver),
   290	            Box::new(PiDriver),
   291	            Box::new(AntigravityDriver),
   292	            Box::new(HermesDriver),
   293	        ];
   294	        for driver in &drivers {
   295	            let results = driver.test_contract();
   296	            assert!(
   297	                !results.is_empty(),
   298	                "{} has no conformance cases",
   299	                driver.name()
   300	            );
   301	            for result in &results {
   302	                assert!(
   303	                    result.passed,
   304	                    "{} failed conformance case {:?}",
   305	                    driver.name(),
   306	                    result.name
   307	                );
   308	            }
   309	        }
   310	    }
   311	
   312	    /// F6: the Antigravity enrollment is PROVEN by a uniquely-named test —
   313	    /// the generic `conformance` filter matched two pre-existing tests and
   314	    /// would pass with zero Antigravity code, so the enrollment needed a name
   315	    /// that can only match this one. Asserts the hardcoded array is now 5
   316	    /// drivers AND that the Antigravity driver passes all 7 contract checks.
   317	    #[test]
   318	    fn antigravity_conformance_enrollment() {
   319	        let drivers: [Box<dyn AgentDriver>; 5] = [
   320	            Box::new(ClaudeDriver),
   321	            Box::new(CodexDriver),
   322	            Box::new(OpenCodeDriver),
   323	            Box::new(PiDriver),
   324	            Box::new(AntigravityDriver),
   325	        ];
   326	        let antigravity = drivers
   327	            .iter()
   328	            .find(|d| d.name() == "Antigravity")
   329	            .expect("the Antigravity driver must be enrolled in the shared suite");
   330	        let results = antigravity.test_contract();
   331	        assert_eq!(
   332	            results.len(),
   333	            7,
   334	            "1 name + 5 per-stage DEVFLOW_RESULT prompts + 1 program"
   335	        );
   336	        for result in &results {
   337	            assert!(
   338	                result.passed,
   339	                "Antigravity failed conformance case {:?}",
   340	                result.name
   341	            );
   342	        }
   343	        // The whole suite still passes with the 5th driver present.
   344	        for driver in &drivers {
   345	            assert!(
   346	                driver.test_contract().iter().all(|r| r.passed),
   347	                "{} must pass the shared conformance suite",
   348	                driver.name()
   349	            );
   350	        }
   351	    }
   352	
   353	    /// Phase 42 Task 2 (D-06): the Hermes enrollment is PROVEN by a uniquely-named test.
   354	    /// Asserts the hardcoded array is now 6 drivers AND that the Hermes driver
   355	    /// passes all 7 contract checks.
   356	    #[test]
   357	    fn hermes_conformance_enrollment() {
   358	        let drivers: [Box<dyn AgentDriver>; 6] = [
   359	            Box::new(ClaudeDriver),
   360	            Box::new(CodexDriver),
   361	            Box::new(OpenCodeDriver),
   362	            Box::new(PiDriver),
   363	            Box::new(AntigravityDriver),
   364	            Box::new(HermesDriver),
   365	        ];
   366	        let hermes = drivers
   367	            .iter()
   368	            .find(|d| d.name() == "Hermes")
   369	            .expect("the Hermes driver must be enrolled in the shared suite");
   370	        let results = hermes.test_contract();
   371	        assert_eq!(
   372	            results.len(),
   373	            7,
   374	            "1 name + 5 per-stage DEVFLOW_RESULT prompts + 1 program"
   375	        );
   376	        for result in &results {
   377	            assert!(
   378	                result.passed,
   379	                "Hermes failed conformance case {:?}",
   380	                result.name
   381	            );
   382	        }
   383	        // The whole suite still passes with the 6th driver present.
   384	        for driver in &drivers {
   385	            assert!(
   386	                driver.test_contract().iter().all(|r| r.passed),
   387	                "{} must pass the shared conformance suite",
   388	                driver.name()
   389	            );
   390	        }
   391	    }
   392	
   393	    /// A deliberately-broken driver: empty render + empty program. The suite
   394	    /// must FAIL it — the negative control proving `test_contract` isn't
   395	    /// vacuous (code-review finding #7).
   396	    struct BrokenDriver;
   397	
   398	    impl AgentDriver for BrokenDriver {
   399	        fn name(&self) -> &'static str {
   400	            "broken"
   401	        }
   402	        fn render_prompt(&self, _intent: &crate::prompt::StageIntent) -> String {
   403	            String::new()
   404	        }
   405	        fn build_command(
   406	            &self,
   407	            _phase: PhaseId,
   408	            _prompt: &str,
   409	            _roots: &[PathBuf],
   410	        ) -> (&'static str, Vec<String>) {
   411	            ("", Vec::new())
   412	        }
   413	    }
   414	
   415	    #[test]
   416	    fn conformance_suite_fails_a_broken_driver() {
   417	        let results = BrokenDriver.test_contract();
   418	        assert!(
   419	            results.iter().any(|r| !r.passed),
   420	            "the conformance suite must fail a broken driver (empty render, empty program)"
   421	        );
   422	    }
   423	
   424	    /// The workflow renderer must preserve the per-stage contracts (code-review
   425	    /// findings #1-5): Validate verdict, Ship review gate, Define no-op, Plan
   426	    /// idempotency, and a per-driver workflow root.
   427	    #[test]
   428	    fn workflow_render_preserves_stage_contracts() {
   429	        use crate::prompt::StageIntent;
   430	        use crate::stage::Stage;
   431	
   432	        let codex = CodexDriver;
   433	
   434	        // Validate demands the verdict (finding #1).
   435	        let validate =
   436	            codex.render_prompt(&StageIntent::for_stage(Stage::Validate, PhaseId::new(7)));
   437	        assert!(validate.contains("\"verdict\": \"pass\""));
   438	        assert!(validate.contains("\"verdict\": \"gaps\""));
   439	
   440	        // Ship keeps the review gate (finding #2).
   441	        let ship = codex.render_prompt(&StageIntent::for_stage(Stage::Ship, PhaseId::new(7)));
   442	        assert!(ship.contains("Critical"));
   443	        assert!(ship.contains("review:"));
   444	
   445	        // Define is the D-14 no-op (finding #3).
   446	        let define = codex.render_prompt(&StageIntent::for_stage(Stage::Define, PhaseId::new(7)));
   447	        assert!(define.contains("must NOT run") || define.contains("do NOT run"));
   448	        assert!(!define.contains("discuss-phase.md"));
   449	
   450	        // Plan keeps the idempotency guard (finding #3).
   451	        let plan = codex.render_prompt(&StageIntent::for_stage(Stage::Plan, PhaseId::new(7)));
   452	        assert!(plan.contains("already exists"));
   453	
   454	        // Pi points at its own workflow root (finding #5).
   455	        let pi_code = PiDriver.render_prompt(&StageIntent::for_stage(Stage::Code, PhaseId::new(7)));
   456	        assert!(pi_code.contains("$HOME/.pi/agent/gsd-core/workflows"));
   457	        assert!(!pi_code.contains("$HOME/.codex/gsd-core"));
   458	    }
   459	
   460	    #[test]
   461	    fn codex_define_and_plan_require_an_existing_artifact() {
   462	        assert_eq!(
   463	            CodexDriver.interactivity_mode(crate::stage::Stage::Define),
   464	            InteractivityMode::RequiresExistingArtifact
   465	        );
   466	        assert_eq!(
   467	            CodexDriver.interactivity_mode(crate::stage::Stage::Plan),
   468	            InteractivityMode::RequiresExistingArtifact
   469	        );
   470	        assert_eq!(
   471	            CodexDriver.interactivity_mode(crate::stage::Stage::Code),
   472	            InteractivityMode::HeadlessSafe
   473	        );
   474	        assert_eq!(
   475	            ClaudeDriver.interactivity_mode(crate::stage::Stage::Define),
   476	            InteractivityMode::HeadlessSafe
   477	        );
   478	    }
   479	
   480	    /// The shared-prompt invariant is retired (999.31 / 37-01): Claude and
   481	    /// OpenCode still render byte-identical legacy text, but Codex now renders
   482	    /// a Codex-native instruction instead of the shared `/gsd-*` slash command.
   483	    #[test]
   484	    fn claude_and_opencode_stay_identical_but_codex_renders_native() {
   485	        let intent = crate::prompt::StageIntent::for_stage(Stage::Code, PhaseId::new(7));
   486	        let claude = driver_for(AgentKind::Claude).render_prompt(&intent);
   487	        let opencode = driver_for(AgentKind::OpenCode).render_prompt(&intent);
   488	        let codex = driver_for(AgentKind::Codex).render_prompt(&intent);
   489	
   490	        // Claude/OpenCode: byte-identical legacy text (zero regression).
   491	        assert_eq!(
   492	            claude, opencode,
   493	            "Claude and OpenCode must stay byte-identical after the migration"
   494	        );
   495	        assert_eq!(
   496	            claude,
   497	            stage_prompt(Stage::Code, PhaseId::new(7)),
   498	            "Claude must render the legacy stage_prompt text byte-for-byte (CONTEXT D-01)"
   499	        );
   500	
   501	        // Codex: native, NOT the shared slash-command text (the dogfood fix).
   502	        assert_ne!(
   503	            codex, claude,
   504	            "Codex must no longer render the shared slash-command text"
   505	        );
   506	        // Negative control, precise: no GSD slash COMMAND may appear (the
   507	        // `gsd-core` workflow-directory path is legitimate and must not trip
   508	        // a naive `/gsd-` substring check).
   509	        for command in [
   510	            "/gsd-discuss-phase",
   511	            "/gsd-plan-phase",
   512	            "/gsd-execute-phase",
   513	            "/gsd-validate-phase",
   514	            "/gsd-ship",
   515	            "/gsd-code-review",
   516	            "/gsd-audit-fix",
   517	        ] {
   518	            assert!(
   519	                !codex.contains(command),
   520	                "Codex render must not carry {command}: {codex}"
   521	            );
   522	        }
   523	        // Positive oracle: the native instruction references the workflow path,
   524	        // carries the --auto token, and states the completion contract (so an
   525	        // empty or \"do nothing\" string cannot pass).
   526	        assert!(codex.contains("execute-phase.md"));
   527	        assert!(codex.contains("--auto"));
   528	        assert!(codex.contains("DEVFLOW_RESULT"));
   529	    }
   530	
   531	    /// The Phase 31 launch contract, asserted as one thing because getting
   532	    /// only the flags right is the documented way to half-implement it: the
   533	    /// transport is `stream-json` in BOTH directions, and the prompt is not a
   534	    /// positional argument at all.
   535	    #[test]
   536	    fn claude_launches_headless_stream_json_without_positional_prompt() {
   537	        let prompt = stage_prompt(Stage::Code, PhaseId::new(3));
   538	        let (program, args) =
   539	            driver_for(AgentKind::Claude).build_command(PhaseId::new(3), &prompt, &[]);
   540	        assert_eq!(program, "claude");
   541	        assert!(args.iter().any(|a| a == "-p"));
   542	        assert!(
   543	            args.windows(2)
   544	                .any(|w| w[0] == "--input-format" && w[1] == "stream-json"),
   545	            "the INPUT format is what moves the initial turn onto stdin; \
   546	             flipping only the output format leaves the CLI with no first \
   547	             turn and it stalls headless: {args:?}"
   548	        );
   549	        assert!(
   550	            args.windows(2)
   551	                .any(|w| w[0] == "--output-format" && w[1] == "stream-json"),
   552	            "the OUTPUT format is what makes the capture a JSONL event stream \
   553	             the Layer 1 stream parser can read: {args:?}"
   554	        );
   555	        assert!(args.iter().any(|a| a == "--dangerously-skip-permissions"));
   556	        assert!(
   557	            !args.iter().any(|arg| arg.contains("DEVFLOW_RESULT")),
   558	            "no positional prompt: the initial user turn travels on stdin, \
   559	             written by the monitor: {args:?}"
   560	        );
   561	    }
   562	
   563	    #[test]
   564	    fn codex_wraps_prompt_in_exec_and_json() {
   565	        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
   566	        let (program, args) =
   567	            driver_for(AgentKind::Codex).build_command(PhaseId::new(7), &prompt, &[]);
   568	        assert_eq!(program, "codex");
   569	        let joined = args.join(" ");
   570	        assert!(joined.contains("exec"));
   571	        assert!(joined.contains("--sandbox workspace-write"));
   572	        assert!(joined.contains("--json"));
   573	    }
   574	
   575	    #[test]
   576	    fn opencode_wraps_prompt_in_run() {
   577	        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
   578	        let (program, args) =
   579	            driver_for(AgentKind::OpenCode).build_command(PhaseId::new(7), &prompt, &[]);
   580	        assert_eq!(program, "opencode");
   581	        assert_eq!(args, ["run", prompt.as_str()]);
   582	    }
   583	
   584	    /// 13-06 dogfood regression (Codex leg): linked-worktree git metadata
   585	    /// lives under the main repo's `.git/` — outside the workspace-write
   586	    /// sandbox — and Codex read-only-mounts the cwd's resolved git dir, so
   587	    /// BOTH the common `.git` and the worktree admin dir must be granted
   588	    /// (verified with `codex sandbox` probes). Without roots, no override.
   589	    #[test]
   590	    fn codex_grants_writable_roots_for_worktree_git_metadata() {
   591	        let prompt = stage_prompt(Stage::Code, PhaseId::new(7));
   592	        let roots = vec![
   593	            PathBuf::from("/repo/.git"),
   594	            PathBuf::from("/repo/.git/worktrees/phase-07"),
   595	        ];
   596	        let (_, args) =
   597	            driver_for(AgentKind::Codex).build_command(PhaseId::new(7), &prompt, &roots);
   598	        let joined = args.join(" ");
   599	        assert!(
   600	            joined.contains(
   601	                r#"-c sandbox_workspace_write.writable_roots=["/repo/.git","/repo/.git/worktrees/phase-07"]"#
   602	            ),
   603	            "codex must whitelist the common .git AND the worktree admin dir: {joined}"
   604	        );
   605	
   606	        let (_, args) = driver_for(AgentKind::Codex).build_command(PhaseId::new(7), &prompt, &[]);
   607	        assert!(
   608	            !args.join(" ").contains("writable_roots"),
   609	            "no override without an extra root"
   610	        );
   611	    }
   612	
   613	    /// 13-06 dogfood regression: signed commits fail inside the Codex
   614	    /// sandbox (no route to the operator's signing agent) — codex scopes an
   615	    /// unsigned-commit override to its own process tree via GIT_CONFIG_*
   616	    /// env; agents without a sandbox get no extra env.
   617	    #[test]
   618	    fn codex_disables_signing_via_env_others_do_not() {
   619	        let env = driver_for(AgentKind::Codex).environment();
   620	        assert!(env.contains(&("GIT_CONFIG_KEY_0".into(), "commit.gpgsign".into())));
   621	        assert!(env.contains(&("GIT_CONFIG_KEY_1".into(), "tag.gpgsign".into())));
   622	        assert!(driver_for(AgentKind::Claude).environment().is_empty());
   623	        assert!(driver_for(AgentKind::OpenCode).environment().is_empty());
   624	    }
   625	
   626	    /// D-13: `preflight`'s default body is `Ok(())` for every built-in
   627	    /// adapter — none of Claude/Codex/OpenCode override it in Phase 17 (no
   628	    /// reviewer-set storage exists yet in `state.rs`/`config.rs`, review
   629	    /// consensus #6).
   630	    #[test]
   631	    fn default_preflight_is_ok_for_built_in_adapters() {
   632	        let state = crate::state::State::new(
   633	            PhaseId::new(1),
   634	            AgentKind::Claude,
   635	            crate::mode::Mode::Auto,
   636	            PathBuf::from("/repo"),
   637	        );
   638	        assert!(driver_for(AgentKind::Claude).health(&state).is_ok());
   639	        assert!(driver_for(AgentKind::Codex).health(&state).is_ok());
   640	        assert!(driver_for(AgentKind::OpenCode).health(&state).is_ok());
   641	    }
   642	}
     1	//! DevFlow state machine.
     2	//!
     3	//! Drives the development workflow through a single linear chain of five stages:
     4	//! Define → Plan → Code → Validate → Ship. See [`crate::stage::Stage`].
     5	
     6	use serde::{Deserialize, Serialize};
     7	use std::fmt;
     8	use std::path::PathBuf;
     9	use std::str::FromStr;
    10	use std::time::{SystemTime, UNIX_EPOCH};
    11	
    12	use crate::mode::Mode;
    13	use crate::phase_id::PhaseId;
    14	use crate::stage::Stage;
    15	
    16	/// Full workflow state persisted to `.devflow/state.json`.
    17	///
    18	/// # Construction
    19	///
    20	/// Marked `#[non_exhaustive]`: downstream crates must build this through
    21	/// [`State::new`] and then assign the fields they care about, rather than by
    22	/// struct literal. Deserialization is unaffected — the `Deserialize` derive
    23	/// and every `#[serde(default)]` field keep working exactly as before, so
    24	/// state files written by older binaries still load.
    25	///
    26	/// This exists because `State` accumulates a field roughly every phase that
    27	/// adds a run-scoped concept (`worktree_path`, `monitor_pid`, `stop_until`,
    28	/// `yes_ship`, and — in phase 28 — `session_id` and `checkpoint_resumes`).
    29	/// Without `non_exhaustive`, each of those additions is a semver-breaking
    30	/// change for any consumer that used a struct literal, which would force a
    31	/// major bump for what is really an internal bookkeeping change. Paying that
    32	/// cost once here makes every future field additive.
    33	#[derive(Debug, Clone, Serialize, Deserialize)]
    34	#[non_exhaustive]
    35	pub struct State {
    36	    /// Current workflow stage.
    37	    pub stage: Stage,
    38	    /// Phase number being worked on.
    39	    pub phase: PhaseId,
    40	    /// Which coding agent was launched.
    41	    pub agent: AgentKind,
    42	    /// How the pipeline is driven (auto vs. supervise).
    43	    pub mode: Mode,
    44	    /// Whether a gate has been written and is awaiting a human response.
    45	    #[serde(default)]
    46	    pub gate_pending: bool,
    47	    /// Consecutive Validate failures — drives the Auto-mode forced gate after
    48	    /// [`crate::mode::MAX_CONSECUTIVE_FAILURES`] failures. Persisted across
    49	    /// `devflow advance` invocations so the counter survives monitor restarts.
    50	    #[serde(default)]
    51	    pub consecutive_failures: u32,
    52	    /// Consecutive infrastructure-class faults (`ResourceKilled`,
    53	    /// `AgentUnavailable`) — distinct from [`Self::consecutive_failures`]
    54	    /// (D-08, 17-01). Gates at [`crate::mode::MAX_INFRA_FAILURES`]. Any
    55	    /// increment (wired in Plan 04) must use `saturating_add` so a
    56	    /// long-running stuck loop cannot overflow `u32`. A serde-absent value
    57	    /// (older persisted state) defaults to 0. Reset to 0 on every successful
    58	    /// stage transition, alongside `consecutive_failures` (CR-01, 17-06 gap
    59	    /// closure), so the ceiling bounds a stuck loop, not a phase's lifetime.
    60	    #[serde(default)]
    61	    pub infra_failures: u32,
    62	    /// How many times a preflight gate has been resolved and retried for
    63	    /// this phase (18f). Bounded by [`crate::mode::MAX_PREFLIGHT_RETRIES`].
    64	    /// Persisted rather than recursion-scoped because the documented wedge
    65	    /// spanned separate `devflow` invocations after a monitor death — an
    66	    /// in-process recursion-depth counter would reset to zero on every new
    67	    /// process and fail to bound the exact incident it exists to prevent.
    68	    /// Reset to 0 whenever preflight passes and whenever a human explicitly
    69	    /// approves (`GateAction::Advance`), both inside `run_preflight`. Unlike
    70	    /// [`Self::consecutive_failures`] and [`Self::infra_failures`], this
    71	    /// counter is NOT touched by `transition()`.
    72	    #[serde(default)]
    73	    pub preflight_retries: u32,
    74	    /// The commit count observed on the phase's feature branch at the most
    75	    /// recent Validate failure (999.66, D-03) — the forward-progress
    76	    /// baseline [`crate::mode::consecutive_failures_made_progress`] compares
    77	    /// against to decide whether a new failure begins a fresh streak or
    78	    /// continues the existing one.
    79	    ///
    80	    /// `None` means no prior failure has been recorded — either the first
    81	    /// failure of a phase, or the first failure observed after resuming
    82	    /// state written by a binary predating this field — and is deliberately
    83	    /// distinct from `Some(0)`, which means a failure WAS recorded and the
    84	    /// branch genuinely carried zero commits at that moment; a later failure
    85	    /// that again counts zero commits must accumulate against that `Some(0)`
    86	    /// baseline rather than being treated as a fresh streak.
    87	    ///
    88	    /// A serde-absent value (state written by a binary predating this field)
    89	    /// deserializes to `None`, which is exactly the "no prior record"
    90	    /// meaning above — the same backward-compat pattern as every other
    91	    /// `#[serde(default)]` field added since 17-01.
    92	    ///
    93	    /// Unlike [`Self::consecutive_failures`] and [`Self::infra_failures`],
    94	    /// this field is NOT touched by `transition()` — it is a baseline
    95	    /// observation rather than a counter, matching how
    96	    /// [`Self::preflight_retries`] and [`Self::checkpoint_resumes`] are
    97	    /// handled. It is replaced wholesale at each failure rather than
    98	    /// incremented, so it needs no `saturating_add` treatment, unlike every
    99	    /// other numeric field on this struct.
   100	    #[serde(default)]
   101	    pub last_validate_failure_commit_count: Option<u32>,
   102	    /// Every Validate failure recorded for this PHASE, accumulated without
   103	    /// regard to forward progress (999.78/WR-01, D-07) — the backstop bound
   104	    /// [`crate::mode::MAX_PHASE_VALIDATE_FAILURES`] compares against, and the
   105	    /// leading number in the Supervise gate message (WR-04).
   106	    ///
   107	    /// A serde-absent value (state written by a binary predating this field)
   108	    /// deserializes to 0, which is exactly its "no failures recorded for this
   109	    /// phase" meaning — the same backward-compat pattern as every other
   110	    /// `#[serde(default)]` field added since 17-01. Unlike
   111	    /// [`Self::last_validate_failure_commit_count`], zero is not ambiguous
   112	    /// here: an upgraded binary and a genuine first failure both start the
   113	    /// budget at its full width, and that widening is what IN-02's distinct
   114	    /// loop-back reason exists to announce.
   115	    ///
   116	    /// Why it exists next to [`Self::consecutive_failures`] rather than
   117	    /// replacing it: `consecutive_failures` is reset whenever
   118	    /// [`crate::mode::consecutive_failures_made_progress`] reports that new
   119	    /// commits landed, and the Code stage's fix command is a GSD command
   120	    /// which routinely commits `.planning/` artifacts even when no source
   121	    /// changed. A loop that commits something trivial every cycle therefore
   122	    /// resets the streak every cycle and never reaches
   123	    /// [`crate::mode::MAX_CONSECUTIVE_FAILURES`]. This total cannot be reset
   124	    /// by a commit count.
   125	    ///
   126	    /// **Lifetime — deliberately unlike every other counter on this struct.**
   127	    /// It is NOT touched by the stage transition (`transition_resets_*` has no
   128	    /// say over it), matching how [`Self::preflight_retries`] and
   129	    /// [`Self::checkpoint_resumes`] are handled, because it is a per-phase
   130	    /// total rather than a per-streak counter. It is also carried across a
   131	    /// forced restart: `commands::start()` reads any persisted state for the
   132	    /// same phase and copies this one field into the fresh `State`, because a
   133	    /// bound a `devflow start --force` resets does not bound the unattended
   134	    /// case D-07 exists for. Exactly two events reset it to zero:
   135	    ///
   136	    /// 1. **Phase completion** — `finish_workflow_with_gate_timeout` calls
   137	    ///    `workflow::clear_state`, deleting `.devflow/state-{NN}.json`, so the
   138	    ///    next start for that phase finds nothing to carry.
   139	    /// 2. **Operator approval at the ceiling gate** — the Validate gate
   140	    ///    handling zeroes it when a human advances or loops back AND
   141	    ///    [`crate::mode::phase_failure_ceiling_reached`] is true. Keyed on that
   142	    ///    predicate and never on "a gate fired": Supervise gates on every
   143	    ///    Validate, so a gate-keyed reset would clear the total at every
   144	    ///    failure and it would never accumulate in the one mode where an
   145	    ///    operator watches every occurrence.
   146	    ///
   147	    /// Any increment must use `saturating_add`, like [`Self::infra_failures`]
   148	    /// and [`Self::checkpoint_resumes`], so an exhausted budget can never wrap
   149	    /// back to zero and silently restore itself.
   150	    #[serde(default)]
   151	    pub phase_validate_failures: u32,
   152	    /// The content fingerprint of this phase's `{N}-VERIFICATION.md` as it
   153	    /// stood at the START of this run (999.79), read via
   154	    /// [`crate::agent_result::phase_verification_fingerprint`] once the
   155	    /// evidence root for the run is known.
   156	    ///
   157	    /// `None` means no artifact was observed at the start of this run — the
   158	    /// ordinary case for a phase being executed for the first time. It is
   159	    /// deliberately distinct from `Some(h)`: an artifact that EXISTS now where
   160	    /// the baseline recorded none was authored during this run, whereas an
   161	    /// artifact whose fingerprint still equals the baseline was inherited from
   162	    /// a previous run and its verdict must not be reused.
   163	    ///
   164	    /// **State written by a binary predating this field also deserializes to
   165	    /// `None`, and that is NOT the same reading** (WR-05, 35-REVIEW). This doc
   166	    /// comment used to claim it was. For a phase started under an older binary
   167	    /// and continued by this one, the previous run's committed
   168	    /// `{N}-VERIFICATION.md` is already on disk while the baseline reads
   169	    /// `None` — so the `(Some, None)` row would classify an inherited artifact
   170	    /// as authored-this-run and dispatch `--gaps-only` against zero matching
   171	    /// plans, gating unresolvably. That is verbatim the DOGFOOD-01-class stall
   172	    /// 999.79 exists to close, reproduced for every in-flight phase across the
   173	    /// upgrade.
   174	    ///
   175	    /// [`Self::verification_baseline_captured`] is the discriminator: only a
   176	    /// run that actually performed the observation sets it, so a `None` from an
   177	    /// old state file is distinguishable from a `None` that means "looked, and
   178	    /// there was nothing there".
   179	    ///
   180	    /// Why this exists at all: nothing deletes or dates `{N}-VERIFICATION.md`,
   181	    /// so a `devflow start --force` re-run checks out a branch still carrying
   182	    /// the previous run's committed copy. Without this baseline the first
   183	    /// Validate failure of that re-run reads the inherited artifact as a
   184	    /// verdict and dispatches a `--gaps-only` pass against zero matching plans,
   185	    /// which gates unresolvably — the same unattended-stall class as
   186	    /// DOGFOOD-01, reached from a different direction.
   187	    ///
   188	    /// **Lifetime.** Like [`Self::last_validate_failure_commit_count`], and
   189	    /// unlike [`Self::consecutive_failures`] and [`Self::infra_failures`], this
   190	    /// field is NOT touched by `transition()` — it is a run-scoped observation
   191	    /// rather than a counter, so it is replaced wholesale rather than
   192	    /// incremented and needs no `saturating_add` treatment. It is also NOT
   193	    /// carried across a forced restart the way
   194	    /// [`Self::phase_validate_failures`] is: a new run must re-observe the
   195	    /// artifact, because the whole point is to compare against what THIS run
   196	    /// started with.
   197	    #[serde(default)]
   198	    pub last_verification_fingerprint: Option<u64>,
   199	    /// Whether [`Self::last_verification_fingerprint`] was actually observed by
   200	    /// this run, as opposed to merely absent (WR-05, 35-REVIEW).
   201	    ///
   202	    /// `Option<u64>` cannot carry this on its own: `None` means both "the run
   203	    /// looked and found no artifact" and "this state file predates the field,
   204	    /// so nobody ever looked", and those two demand OPPOSITE dispatches. The
   205	    /// first is the ordinary first-verification case and `--gaps-only` is
   206	    /// right; the second may be sitting on an inherited artifact, where
   207	    /// `--gaps-only` matches zero plans and stalls.
   208	    ///
   209	    /// `false` is therefore the correct serde default in both directions: a
   210	    /// state file written before this field existed genuinely did not capture a
   211	    /// baseline, and the conservative reading of an artifact whose provenance
   212	    /// is unknown is "inherited" — a full execute is wasteful, an unresolvable
   213	    /// gate is not recoverable.
   214	    ///
   215	    /// Set exactly once per run, at the same site that captures the baseline,
   216	    /// after `state.worktree_path` holds its final value.
   217	    #[serde(default)]
   218	    pub verification_baseline_captured: bool,
   219	    /// The mtime of the same artifact [`Self::last_verification_fingerprint`]
   220	    /// hashes, in nanoseconds since the Unix epoch, as of the same observation.
   221	    ///
   222	    /// WR-06 (35-REVIEW): a content fingerprint cannot see an IDEMPOTENT
   223	    /// rewrite. A Validate agent that re-authors byte-identical content on a
   224	    /// later failing cycle produces the same hash as an artifact nobody
   225	    /// touched, so a hash-only rule reads its own agent's work as inherited and
   226	    /// dispatches a full execute — re-running every plan in the phase on every
   227	    /// subsequent cycle instead of the gaps-only pass Phase 33 built. That is
   228	    /// the "too strict" direction the freshness rule's own comment claims to
   229	    /// guard against and did not.
   230	    ///
   231	    /// Moves in lockstep with the fingerprint: written at the same capture
   232	    /// site, replaced at the same update site, and never read on its own — the
   233	    /// pair is the observation, and either one differing means the artifact was
   234	    /// written during this run.
   235	    ///
   236	    /// 35.2 D-05: mtime was considered as the provenance signal and REJECTED.
   237	    /// A branch checkout or worktree merge-back updates mtime exactly as a
   238	    /// real write does — it fails on the identical scenario
   239	    /// [`Self::verification_run_nonce`] exists to catch, which is why 999.89
   240	    /// survived 35-05's WR-06 fix. mtime is still what detects a byte-identical
   241	    /// rewrite INSIDE the Validate dispatch window whose bounds the nonce
   242	    /// establishes — provenance and freshness are different questions.
   243	    #[serde(default)]
   244	    pub last_verification_mtime_nanos: Option<u64>,
   245	    /// A run-owned marker stamped per Validate dispatch proving DevFlow itself
   246	    /// launched the agent whose output this state describes (35.2, 999.89 /
   247	    /// HARDEN-03, D-01).
   248	    ///
   249	    /// `None` means DevFlow never stamped a Validate dispatch for this state,
   250	    /// which is both the pre-35.2-state-file case and the never-dispatched
   251	    /// case. Both demand the conservative reading: the artifact's provenance is
   252	    /// unknown and `verification_authored_this_run` returns `false`.
   253	    ///
   254	    /// **Lifetime — replaced wholesale on every Validate dispatch, not
   255	    /// incremented across runs.** Unlike [`Self::consecutive_failures`] and
   256	    /// [`Self::phase_validate_failures`], this field is NOT touched by
   257	    /// `transition()`, and [`State::new`] resets it, so a `--force` restart
   258	    /// cannot inherit a previous run's stamp. The value is a monotonically
   259	    /// increasing counter; the predicate consults [`Option::is_some`], never
   260	    /// the magnitude, so saturation cannot degrade the signal.
   261	    ///
   262	    /// The write site is `launch_stage_inner` in `pipeline_launch.rs`, gated
   263	    /// on `Stage::Validate`, co-located with a fresh fingerprint/mtime
   264	    /// re-observation — the stamp and the baseline are one mechanism, and
   265	    /// splitting them silently restores the run-wide observation window.
   266	    ///
   267	    /// An actor who can write `.devflow/state-{N}.json` can set `stage` or
   268	    /// `consecutive_failures` directly; this field adds no attack surface
   269	    /// beyond what already exists (P-03).
   270	    #[serde(default)]
   271	    pub verification_run_nonce: Option<u64>,
   272	    /// When the phase started (Unix seconds).
   273	    pub started_at: String,
   274	    /// Path to the project root.
   275	    pub project_root: PathBuf,
   276	    /// Working directory for the agent when running in a git worktree.
   277	    ///
   278	    /// `None` means the agent runs in `project_root`. State and capture files
   279	    /// always live under the main `project_root`; only the agent's cwd changes.
   280	    #[serde(default)]
   281	    pub worktree_path: Option<PathBuf>,
   282	    /// PID of the detached monitor process that owns the agent for the
   283	    /// current stage, recorded by `launch_stage` at spawn time. `None` means
   284	    /// no monitor has been spawned for this state yet, OR the state was
   285	    /// written by a binary predating this field — in both cases the
   286	    /// liveness probe reports Unknown, never Stuck.
   287	    #[serde(default)]
   288	    pub monitor_pid: Option<u32>,
   289	    /// The Claude session id captured from the most recent captured stdout
   290	    /// envelope for this phase's current stage (D-04, 28-02), read via
   291	    /// [`crate::agent_result::session_id_from_capture`]. `None` means EITHER
   292	    /// "no session has been captured for this state yet" OR "the state was
   293	    /// written by a binary predating this field" — both cases behave
   294	    /// identically (no relaunch target to address). Recorded so a checkpoint
   295	    /// auto-decide relaunch (plan 28-03) can `--resume` the exact session
   296	    /// that hit the checkpoint rather than spawning a fresh one, which would
   297	    /// lose the original session's conversation context and permission mode.
   298	    #[serde(default)]
   299	    pub session_id: Option<String>,
   300	    /// How many times the current stage's agent has been relaunched via a
   301	    /// checkpoint auto-decide resume (D-04, 28-03). Bounds a stuck
   302	    /// checkpoint loop against `mode::MAX_CHECKPOINT_RESUMES` (added in plan
   303	    /// 28-03) the same way [`Self::infra_failures`] bounds an infra-fault
   304	    /// loop against `mode::MAX_INFRA_FAILURES`. Reset to 0 by every ordinary fresh stage
   305	    /// launch, so the ceiling bounds one stage's resume budget, not a
   306	    /// phase's lifetime (the same distinction `MAX_INFRA_FAILURES`' doc
   307	    /// comment draws for `infra_failures`). Any increment must use
   308	    /// `saturating_add` so a stuck loop cannot overflow `u32`. A
   309	    /// serde-absent value (state written by a binary predating this field)
   310	    /// defaults to 0.
   311	    #[serde(default)]
   312	    pub checkpoint_resumes: u32,
   313	    /// The stage `devflow start --until <stage>` requests as the last stage
   314	    /// to run before halting (20c). `None` means no stop point was
   315	    /// requested (the pipeline runs to Ship), OR the state was written by a
   316	    /// binary predating this field — both cases behave identically (no
   317	    /// interception in `transition()`).
   318	    #[serde(default)]
   319	    pub stop_until: Option<Stage>,
   320	    /// Set by `transition()` when `stop_until` names the stage just
   321	    /// completed — a terminal-but-not-failed halt short of Ship (20c).
   322	    /// `false` for a normal in-flight or completed-to-Ship phase, and for
   323	    /// any state written by a binary predating this field.
   324	    #[serde(default)]
   325	    pub stopped: bool,
   326	    /// Human-readable reason recorded alongside `stopped` (20c). `None`
   327	    /// when `stopped` is `false`, or when the state predates this field.
   328	    #[serde(default)]
   329	    pub stop_reason: Option<String>,
   330	    /// Pre-authorization for the Ship gate (D-04/D-05/D-06, 23-09),
   331	    /// set only from the `--yes-ship` CLI flag typed on `devflow start`.
   332	    ///
   333	    /// Persisted rather than passed through the call stack: the Ship gate
   334	    /// fires inside a detached monitor's `advance` process, minutes to
   335	    /// hours after the launching `devflow start` process has already
   336	    /// exited, so a CLI-scoped value would be gone by the time it matters —
   337	    /// only a value written to `state.json` at start time survives to be
   338	    /// read back by that later, separate process. `false` for any state
   339	    /// written by a binary predating this field.
   340	    #[serde(default)]
   341	    pub yes_ship: bool,
   342	    /// What this run's delivery canary established (D-13/D-15, 31-03),
   343	    /// recorded by the first stage launch that routes through the Claude
   344	    /// `stream-json` transport. `None` means EITHER "no canary has run for
   345	    /// this run yet" OR "the state was written by a binary predating this
   346	    /// field" — both cases behave identically: the canary runs.
   347	    ///
   348	    /// Persisted rather than held in memory for the same reason
   349	    /// [`Self::yes_ship`] is: each stage launch happens in a SEPARATE
   350	    /// `devflow` process (the monitor's own `advance` tail), so an
   351	    /// in-process flag would reset to "not yet run" at every stage
   352	    /// transition and re-spend a real throwaway agent invocation each time —
   353	    /// which is exactly the symptom 31-RESEARCH Pitfall 5 names for a canary
   354	    /// that landed in the per-stage `preflight` hook.
   355	    ///
   356	    /// A recorded `Absent`/`Unverified` keeps refusing on every later launch
   357	    /// in the run; it is not consumed by the first refusal.
   358	    #[serde(default)]
   359	    pub canary: Option<crate::canary::CanaryOutcome>,
   360	    /// D-11's opt-out: force the pre-31 single-document Claude launch
   361	    /// (positional prompt, `--output-format json`, the `sh` monitor) for this
   362	    /// run, off by default.
   363	    ///
   364	    /// `false` means EITHER "the operator did not ask for the legacy path" OR
   365	    /// "the state was written by a binary predating this field" — both cases
   366	    /// behave identically: the D-09/D-10 rollout decides the transport, which
   367	    /// is the pre-existing behaviour.
   368	    ///
   369	    /// Persisted rather than passed through the call stack for the reason
   370	    /// [`Self::yes_ship`] gives: each stage launch happens in a SEPARATE
   371	    /// `devflow` process (the detached monitor's own `advance` tail), so a
   372	    /// CLI-scoped value would be gone by the time the second stage launches
   373	    /// and the run would silently revert to the stream transport mid-flight.
   374	    ///
   375	    /// Only ever OR-ed, never cleared, once set — see
   376	    /// `pipeline_launch::apply_legacy_launch_opt_out`. Clearing it on a plain
   377	    /// `devflow resume` would be the same silent-drop class as `stop_until`'s
   378	    /// old unconditional clear (999.60). To turn it back off, edit
   379	    /// `.devflow/state-NN.json` or start a new run.
   380	    #[serde(default)]
   381	    pub legacy_claude_launch: bool,
   382	}
   383	
   384	/// Supported coding agents.
   385	#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
   386	#[serde(rename_all = "lowercase")]
   387	pub enum AgentKind {
   388	    /// Anthropic Claude Code CLI.
   389	    Claude,
   390	    /// OpenAI Codex CLI.
   391	    Codex,
   392	    /// OpenCode CLI.
   393	    OpenCode,
   394	    /// Pi coding-agent harness.
   395	    Pi,
   396	    /// Antigravity CLI (`agy`), stream-json transport (phase 41).
   397	    Antigravity,
   398	    /// Hermes Agent CLI (`hermes`), headless oneshot (phase 42).
   399	    Hermes,
   400	}
   401	
   402	impl fmt::Display for AgentKind {
   403	    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
   404	        let name = match self {
   405	            AgentKind::Claude => "claude",
   406	            AgentKind::Codex => "codex",
   407	            AgentKind::OpenCode => "opencode",
   408	            AgentKind::Pi => "pi",
   409	            AgentKind::Antigravity => "antigravity",
   410	            AgentKind::Hermes => "hermes",
   411	        };
   412	        f.write_str(name)
   413	    }
   414	}
   415	
   416	impl FromStr for AgentKind {
   417	    type Err = AgentParseError;
   418	
   419	    fn from_str(value: &str) -> Result<Self, Self::Err> {
   420	        match value.to_ascii_lowercase().as_str() {
   421	            "claude" => Ok(AgentKind::Claude),
   422	            "codex" => Ok(AgentKind::Codex),
   423	            "opencode" | "open-code" => Ok(AgentKind::OpenCode),
   424	            "pi" => Ok(AgentKind::Pi),
   425	            "antigravity" => Ok(AgentKind::Antigravity),
   426	            "hermes" => Ok(AgentKind::Hermes),
   427	            other => Err(AgentParseError(other.to_string())),
   428	        }
   429	    }
   430	}
   431	
   432	/// Error returned when parsing an unsupported agent name.
   433	#[derive(Debug, Clone, thiserror::Error)]
   434	#[error("unsupported agent `{0}`; expected claude, codex, opencode, pi, antigravity, or hermes")]
   435	pub struct AgentParseError(String);
   436	
   437	impl State {
   438	    /// Create a new state for starting a phase at the [`Stage::Define`] stage.
   439	    pub fn new(phase: PhaseId, agent: AgentKind, mode: Mode, project_root: PathBuf) -> Self {
   440	        State {
   441	            stage: Stage::Define,
   442	            phase,
   443	            agent,
   444	            mode,
   445	            gate_pending: false,
   446	            consecutive_failures: 0,
   447	            infra_failures: 0,
   448	            preflight_retries: 0,
   449	            last_validate_failure_commit_count: None,
   450	            phase_validate_failures: 0,
   451	            last_verification_fingerprint: None,
   452	            verification_baseline_captured: false,
   453	            last_verification_mtime_nanos: None,
   454	            verification_run_nonce: None,
   455	            started_at: timestamp_now(),
   456	            project_root,
   457	            worktree_path: None,
   458	            monitor_pid: None,
   459	            session_id: None,
   460	            checkpoint_resumes: 0,
   461	            stop_until: None,
   462	            stopped: false,
   463	            stop_reason: None,
   464	            yes_ship: false,
   465	            canary: None,
   466	            legacy_claude_launch: false,
   467	        }
   468	    }
   469	}
   470	
   471	fn timestamp_now() -> String {
   472	    match SystemTime::now().duration_since(UNIX_EPOCH) {
   473	        Ok(duration) => format!("{}", duration.as_secs()),
   474	        Err(_) => String::from("0"),
   475	    }
   476	}
   477	
   478	#[cfg(test)]
   479	mod tests {
   480	    use super::*;
   481	    use std::path::PathBuf;
   482	
   483	    #[test]
   484	    fn agent_name_and_display() {
   485	        use crate::agents::driver_for;
   486	        assert_eq!(driver_for(AgentKind::Claude).name(), "Claude Code");
   487	        assert_eq!(driver_for(AgentKind::Codex).name(), "OpenAI Codex");
   488	        assert_eq!(driver_for(AgentKind::OpenCode).name(), "OpenCode");
   489	
   490	        assert_eq!(AgentKind::Claude.to_string(), "claude");
   491	        assert_eq!(AgentKind::Codex.to_string(), "codex");
   492	        assert_eq!(AgentKind::OpenCode.to_string(), "opencode");
   493	    }
   494	
   495	    #[test]
   496	    fn agent_from_str_accepts_canonical_and_aliases() {
   497	        assert_eq!("claude".parse::<AgentKind>().unwrap(), AgentKind::Claude);
   498	        assert_eq!("CLAUDE".parse::<AgentKind>().unwrap(), AgentKind::Claude);
   499	        assert_eq!("codex".parse::<AgentKind>().unwrap(), AgentKind::Codex);
   500	        assert_eq!(
   501	            "opencode".parse::<AgentKind>().unwrap(),
   502	            AgentKind::OpenCode
   503	        );
   504	        assert_eq!(
   505	            "open-code".parse::<AgentKind>().unwrap(),
   506	            AgentKind::OpenCode
   507	        );
   508	    }
   509	
   510	    #[test]
   511	    fn agent_from_str_rejects_unknown() {
   512	        let err = "aider".parse::<AgentKind>().unwrap_err();
   513	        assert!(err.to_string().contains("aider"));
   514	    }
   515	
   516	    #[test]
   517	    fn new_state_starts_at_define() {
   518	        let state = State::new(
   519	            PhaseId::new(2),
   520	            AgentKind::Claude,
   521	            Mode::Auto,
   522	            PathBuf::from("/repo"),
   523	        );
   524	        assert_eq!(state.stage, Stage::Define);
   525	        assert_eq!(state.phase, PhaseId::new(2));
   526	        assert_eq!(state.agent, AgentKind::Claude);
   527	        assert_eq!(state.mode, Mode::Auto);
   528	        assert!(!state.gate_pending);
   529	        assert_eq!(state.consecutive_failures, 0);
   530	        assert_eq!(state.infra_failures, 0);
   531	        assert_eq!(state.preflight_retries, 0);
   532	        assert_eq!(state.phase_validate_failures, 0);
   533	        assert!(!state.started_at.is_empty());
   534	        assert_eq!(state.monitor_pid, None);
   535	        assert_eq!(state.stop_until, None);
   536	        assert!(!state.stopped);
   537	        assert_eq!(state.stop_reason, None);
   538	        assert!(!state.yes_ship);
   539	    }
   540	
   541	    #[test]
   542	    fn state_serde_round_trips() {
   543	        let state = State::new(
   544	            PhaseId::new(9),
   545	            AgentKind::Codex,
   546	            Mode::Supervise,
   547	            PathBuf::from("/repo"),
   548	        );
   549	        let json = serde_json::to_string(&state).unwrap();
   550	        let back: State = serde_json::from_str(&json).unwrap();
   551	        assert_eq!(back.phase, PhaseId::new(9));
   552	        assert_eq!(back.agent, AgentKind::Codex);
   553	        assert_eq!(back.stage, Stage::Define);
   554	        assert_eq!(back.mode, Mode::Supervise);
   555	    }
   556	
   557	    #[test]
   558	    fn consecutive_failures_persists_across_advance_calls() {
   559	        let mut state = State::new(
   560	            PhaseId::new(1),
   561	            AgentKind::Claude,
   562	            Mode::Auto,
   563	            PathBuf::from("/repo"),
   564	        );
   565	        state.consecutive_failures = 3;
   566	        let json = serde_json::to_string(&state).unwrap();
   567	        assert!(
   568	            json.contains("consecutive_failures"),
   569	            "consecutive_failures must appear in persisted JSON"
   570	        );
   571	        let loaded: State = serde_json::from_str(&json).unwrap();
   572	        assert_eq!(
   573	            loaded.consecutive_failures, 3,
   574	            "consecutive_failures must round-trip through serde"
   575	        );
   576	    }
   577	
   578	    /// D-08 (17-01): a distinct infra-failure counter round-trips through
   579	    /// serde and its own key appears in the persisted JSON.
   580	    #[test]
   581	    fn infra_failures_round_trips_through_serde() {
   582	        let mut state = State::new(
   583	            PhaseId::new(1),
   584	            AgentKind::Claude,
   585	            Mode::Auto,
   586	            PathBuf::from("/repo"),
   587	        );
   588	        state.infra_failures = 4;
   589	        let json = serde_json::to_string(&state).unwrap();
   590	        assert!(
   591	            json.contains("infra_failures"),
   592	            "infra_failures must appear in persisted JSON"
   593	        );
   594	        let loaded: State = serde_json::from_str(&json).unwrap();
   595	        assert_eq!(
   596	            loaded.infra_failures, 4,
   597	            "infra_failures must round-trip through serde"
   598	        );
   599	    }
   600	
   601	    /// A serde-absent `infra_failures` (older persisted state.json without
   602	    /// the field) must default to 0, not fail to deserialize.
   603	    #[test]
   604	    fn infra_failures_absent_from_json_defaults_to_zero() {
   605	        let json = r#"{
   606	            "stage": "code",
   607	            "phase": 1,
   608	            "agent": "claude",
   609	            "mode": "auto",
   610	            "started_at": "0",
   611	            "project_root": "/repo"
   612	        }"#;
   613	        let loaded: State = serde_json::from_str(json).unwrap();
   614	        assert_eq!(loaded.infra_failures, 0);
   615	    }
   616	
   617	    /// `last_validate_failure_commit_count` round-trips through serde as an
   618	    /// exact `Option<u32>` (999.66, D-03) — its own key appears in the
   619	    /// persisted JSON before the value round-trip is asserted, so a field
   620	    /// accidentally attributed `skip_serializing_if` (which would still pass
   621	    /// a naive in-memory round-trip while never persisting anything) is
   622	    /// caught.
   623	    #[test]
   624	    fn last_validate_failure_commit_count_round_trips_through_serde() {
   625	        let mut state = State::new(
   626	            PhaseId::new(1),
   627	            AgentKind::Claude,
   628	            Mode::Auto,
   629	            PathBuf::from("/repo"),
   630	        );
   631	        state.last_validate_failure_commit_count = Some(3);
   632	        let json = serde_json::to_string(&state).unwrap();
   633	        assert!(
   634	            json.contains("last_validate_failure_commit_count"),
   635	            "last_validate_failure_commit_count must appear in persisted JSON"
   636	        );
   637	        let loaded: State = serde_json::from_str(&json).unwrap();
   638	        assert_eq!(
   639	            loaded.last_validate_failure_commit_count,
   640	            Some(3),
   641	            "last_validate_failure_commit_count must round-trip through serde"
   642	        );
   643	    }
   644	
   645	    /// A serde-absent `last_validate_failure_commit_count` (state written by
   646	    /// a binary predating this field) must deserialize to `None` — the
   647	    /// "no prior failure recorded" meaning — not to `Some(0)`, which would
   648	    /// misrepresent a never-observed baseline as an observed zero.
   649	    #[test]
   650	    fn last_validate_failure_commit_count_absent_from_json_defaults_to_none() {
   651	        let json = r#"{
   652	            "stage": "code",
   653	            "phase": 1,
   654	            "agent": "claude",
   655	            "mode": "auto",
   656	            "started_at": "0",
   657	            "project_root": "/repo"
   658	        }"#;
   659	        let loaded: State = serde_json::from_str(json).unwrap();
   660	        assert_eq!(loaded.last_validate_failure_commit_count, None);
   661	    }
   662	
   663	    /// 999.78/D-07: `phase_validate_failures` round-trips through serde. The
   664	    /// key-presence assertion comes BEFORE the value round-trip deliberately —
   665	    /// a field that never actually persists still passes a naive in-memory
   666	    /// round trip, and a bound that lives only in memory does not bound a
   667	    /// phase whose whole failure mode spans separate `devflow` processes.
   668	    #[test]
   669	    fn phase_validate_failures_round_trips_through_serde() {
   670	        let mut state = State::new(
   671	            PhaseId::new(1),
   672	            AgentKind::Claude,
   673	            Mode::Auto,
   674	            PathBuf::from("/repo"),
   675	        );
   676	        state.phase_validate_failures = 7;
   677	        let json = serde_json::to_string(&state).unwrap();
   678	        assert!(
   679	            json.contains("phase_validate_failures"),
   680	            "phase_validate_failures must appear in persisted JSON"
   681	        );
   682	        let loaded: State = serde_json::from_str(&json).unwrap();
   683	        assert_eq!(
   684	            loaded.phase_validate_failures, 7,
   685	            "phase_validate_failures must round-trip through serde"
   686	        );
   687	    }
   688	
   689	    /// A serde-absent `phase_validate_failures` (state written by a binary
   690	    /// predating this field) deserializes to 0 — "no failures recorded for
   691	    /// this phase" — rather than failing the load outright, which would make
   692	    /// an upgrade mid-phase unrecoverable.
   693	    #[test]
   694	    fn phase_validate_failures_absent_from_json_defaults_to_zero() {
   695	        let json = r#"{
   696	            "stage": "code",
   697	            "phase": 1,
   698	            "agent": "claude",
   699	            "mode": "auto",
   700	            "started_at": "0",
   701	            "project_root": "/repo"
   702	        }"#;
   703	        let loaded: State = serde_json::from_str(json).unwrap();
   704	        assert_eq!(loaded.phase_validate_failures, 0);
   705	    }
   706	
   707	    /// 999.79 (35-05): `last_verification_fingerprint` round-trips through
   708	    /// serde. The key-presence assertion comes BEFORE the value round-trip for
   709	    /// the same reason the two fields above give — this baseline is written by
   710	    /// `devflow start` and compared by a later `devflow advance`, which is a
   711	    /// different process, so a field that never reaches disk would leave every
   712	    /// comparison reading `None` and defeat the whole rule.
   713	    #[test]
   714	    fn last_verification_fingerprint_round_trips_through_serde() {
   715	        let mut state = State::new(
   716	            PhaseId::new(1),
   717	            AgentKind::Claude,
   718	            Mode::Auto,
   719	            PathBuf::from("/repo"),
   720	        );
   721	        state.last_verification_fingerprint = Some(0x0123_4567_89ab_cdef);
   722	        let json = serde_json::to_string(&state).unwrap();
   723	        assert!(
   724	            json.contains("last_verification_fingerprint"),
   725	            "last_verification_fingerprint must appear in persisted JSON"
   726	        );
   727	        let loaded: State = serde_json::from_str(&json).unwrap();
   728	        assert_eq!(
   729	            loaded.last_verification_fingerprint,
   730	            Some(0x0123_4567_89ab_cdef),
   731	            "last_verification_fingerprint must round-trip through serde"
   732	        );
   733	    }
   734	
   735	    /// A serde-absent `last_verification_fingerprint` (state written by a
   736	    /// binary predating this field) deserializes to `None` — "no artifact was
   737	    /// observed at the start of this run" — rather than failing the load, which
   738	    /// would make an upgrade mid-phase unrecoverable.
   739	    #[test]
   740	    fn last_verification_fingerprint_absent_from_json_defaults_to_none() {
   741	        let json = r#"{
   742	            "stage": "code",
   743	            "phase": 1,
   744	            "agent": "claude",
   745	            "mode": "auto",
   746	            "started_at": "0",
   747	            "project_root": "/repo"
   748	        }"#;
   749	        let loaded: State = serde_json::from_str(json).unwrap();
   750	        assert_eq!(loaded.last_verification_fingerprint, None);
   751	        // WR-05 (35-REVIEW): the SAME absent JSON must also report that nobody
   752	        // captured a baseline. `None` alone cannot carry that — it means both
   753	        // "looked, found nothing" and "never looked" — and the two demand
   754	        // opposite dispatches downstream.
   755	        assert!(
   756	            !loaded.verification_baseline_captured,
   757	            "state predating the baseline field never captured one, and must not claim to"
   758	        );
   759	    }
   760	
   761	    /// The other half of the pair above: a state file written by THIS binary
   762	    /// carries the flag, so the two cases really are distinguishable after a
   763	    /// round trip. Without this, `verification_baseline_captured` could be
   764	    /// hardcoded `false` and the absent-JSON assertion above would still pass.
   765	    #[test]
   766	    fn verification_baseline_captured_round_trips_through_serde() {
   767	        let mut state = State::new(
   768	            PhaseId::new(1),
   769	            AgentKind::Claude,
   770	            Mode::Auto,
   771	            PathBuf::from("/repo"),
   772	        );
   773	        state.verification_baseline_captured = true;
   774	        let json = serde_json::to_string(&state).unwrap();
   775	        assert!(
   776	            json.contains("verification_baseline_captured"),
   777	            "verification_baseline_captured must appear in persisted JSON"
   778	        );
   779	        let loaded: State = serde_json::from_str(&json).unwrap();
   780	        assert!(
   781	            loaded.verification_baseline_captured,
   782	            "a captured baseline must survive the save/load the real pipeline performs"
   783	        );
   784	    }
   785	
   786	    /// 35.2 D-01: verification_run_nonce must survive the save/load cycle
   787	    /// `handle_validate_outcome` → `select_loop_back_fix` performs.
   788	    #[test]
   789	    fn verification_run_nonce_round_trips_through_serde() {
   790	        let mut state = State::new(
   791	            PhaseId::new(1),
   792	            AgentKind::Claude,
   793	            Mode::Auto,
   794	            PathBuf::from("/repo"),
   795	        );
   796	        state.verification_run_nonce = Some(42);
   797	        let json = serde_json::to_string(&state).unwrap();
   798	        assert!(
   799	            json.contains("verification_run_nonce"),
   800	            "verification_run_nonce must appear in persisted JSON"
   801	        );
   802	        let loaded: State = serde_json::from_str(&json).unwrap();
   803	        assert_eq!(
   804	            loaded.verification_run_nonce,
   805	            Some(42),
   806	            "verification_run_nonce must round-trip through serde"
   807	        );
   808	    }
   809	
   810	    /// 35.2 D-01: a serde-absent verification_run_nonce (state written by
   811	    /// a pre-35.2 binary) deserializes to None — the conservative direction.
   812	    #[test]
   813	    fn verification_run_nonce_absent_from_json_defaults_to_none() {
   814	        let json = r#"{
   815	            "stage": "code",
   816	            "phase": 1,
   817	            "agent": "claude",
   818	            "mode": "auto",
   819	            "started_at": "0",
   820	            "project_root": "/repo"
   821	        }"#;
   822	        let loaded: State = serde_json::from_str(json).unwrap();
   823	        assert_eq!(
   824	            loaded.verification_run_nonce, None,
   825	            "pre-35.2 state must default to None — the conservative provenance reading"
   826	        );
   827	    }
   828	
   829	    /// D-18f: `preflight_retries` round-trips through serde (its own key
   830	    /// appears in the persisted JSON) — the wedge this counter bounds spans
   831	    /// separate `devflow` invocations, so it must survive a save/load
   832	    /// cycle, not just live in memory — and a serde-absent value (state
   833	    /// written by a pre-18f binary) deserializes to 0, not a hard error.
   834	    #[test]
   835	    fn preflight_retries_round_trips_through_serde() {
   836	        let mut state = State::new(
   837	            PhaseId::new(1),
   838	            AgentKind::Claude,
   839	            Mode::Auto,
   840	            PathBuf::from("/repo"),
   841	        );
   842	        state.preflight_retries = 2;
   843	        let json = serde_json::to_string(&state).unwrap();
   844	        assert!(
   845	            json.contains("preflight_retries"),
   846	            "preflight_retries must appear in persisted JSON"
   847	        );
   848	        let loaded: State = serde_json::from_str(&json).unwrap();
   849	        assert_eq!(
   850	            loaded.preflight_retries, 2,
   851	            "preflight_retries must round-trip through serde"
   852	        );
   853	
   854	        let absent_json = r#"{
   855	            "stage": "code",
   856	            "phase": 1,
   857	            "agent": "claude",
   858	            "mode": "auto",
   859	            "started_at": "0",
   860	            "project_root": "/repo"
   861	        }"#;
   862	        let loaded_absent: State = serde_json::from_str(absent_json).unwrap();
   863	        assert_eq!(loaded_absent.preflight_retries, 0);
   864	    }
   865	
   866	    /// `monitor_pid` round-trips through serde as an exact `u32` (18b).
   867	    #[test]
   868	    fn monitor_pid_round_trips_through_serde() {
   869	        let mut state = State::new(
   870	            PhaseId::new(1),
   871	            AgentKind::Claude,
   872	            Mode::Auto,
   873	            PathBuf::from("/repo"),
   874	        );
   875	        state.monitor_pid = Some(4242);
   876	        let json = serde_json::to_string(&state).unwrap();
   877	        assert!(
   878	            json.contains("monitor_pid"),
   879	            "monitor_pid must appear in persisted JSON"
   880	        );
   881	        let loaded: State = serde_json::from_str(&json).unwrap();
   882	        assert_eq!(
   883	            loaded.monitor_pid,
   884	            Some(4242),
   885	            "monitor_pid must round-trip through serde"
   886	        );
   887	    }
   888	
   889	    /// A serde-absent `monitor_pid` (state written by a pre-18b binary) must
   890	    /// deserialize to `None`, not `Some(0)` — a `Some(0)` default would let a
   891	    /// pre-18b state file render as a monitor at pid 0.
   892	    #[test]
   893	    fn monitor_pid_absent_from_json_defaults_to_none() {
   894	        let json = r#"{
   895	            "stage": "code",
   896	            "phase": 1,
   897	            "agent": "claude",
   898	            "mode": "auto",
   899	            "started_at": "0",
   900	            "project_root": "/repo"
   901	        }"#;
   902	        let loaded: State = serde_json::from_str(json).unwrap();
   903	        assert_eq!(loaded.monitor_pid, None);
   904	    }
   905	
   906	    /// `session_id` round-trips through serde as an exact `Option<String>`
   907	    /// (D-04, 28-02) — mirrors the `monitor_pid` pair above.
   908	    #[test]
   909	    fn session_id_round_trips_through_serde() {
   910	        let mut state = State::new(
   911	            PhaseId::new(1),
   912	            AgentKind::Claude,
   913	            Mode::Auto,
   914	            PathBuf::from("/repo"),
   915	        );
   916	        state.session_id = Some("cf29bfec-69e8-45df-a4f3-3da08ab6f66e".to_string());
   917	        let json = serde_json::to_string(&state).unwrap();
   918	        assert!(
   919	            json.contains("session_id"),
   920	            "session_id must appear in persisted JSON"
   921	        );
   922	        let loaded: State = serde_json::from_str(&json).unwrap();
   923	        assert_eq!(
   924	            loaded.session_id.as_deref(),
   925	            Some("cf29bfec-69e8-45df-a4f3-3da08ab6f66e"),
   926	            "session_id must round-trip through serde"
   927	        );
   928	    }
   929	
   930	    /// A serde-absent `session_id` (state written by a pre-28-02 binary) must
   931	    /// deserialize to `None`, not fail to deserialize.
   932	    #[test]
   933	    fn session_id_absent_from_json_defaults_to_none() {
   934	        let json = r#"{
   935	            "stage": "code",
   936	            "phase": 1,
   937	            "agent": "claude",
   938	            "mode": "auto",
   939	            "started_at": "0",
   940	            "project_root": "/repo"
   941	        }"#;
   942	        let loaded: State = serde_json::from_str(json).unwrap();
   943	        assert_eq!(loaded.session_id, None);
   944	    }
   945	
   946	    /// `checkpoint_resumes` round-trips through serde as an exact `u32`
   947	    /// (D-04, 28-02).
   948	    #[test]
   949	    fn checkpoint_resumes_round_trips_through_serde() {
   950	        let mut state = State::new(
   951	            PhaseId::new(1),
   952	            AgentKind::Claude,
   953	            Mode::Auto,
   954	            PathBuf::from("/repo"),
   955	        );
   956	        state.checkpoint_resumes = 2;
   957	        let json = serde_json::to_string(&state).unwrap();
   958	        assert!(
   959	            json.contains("checkpoint_resumes"),
   960	            "checkpoint_resumes must appear in persisted JSON"
   961	        );
   962	        let loaded: State = serde_json::from_str(&json).unwrap();
   963	        assert_eq!(
   964	            loaded.checkpoint_resumes, 2,
   965	            "checkpoint_resumes must round-trip through serde"
   966	        );
   967	    }
   968	
   969	    /// A serde-absent `checkpoint_resumes` (state written by a pre-28-02
   970	    /// binary) must deserialize to `0`, not fail to deserialize.
   971	    #[test]
   972	    fn checkpoint_resumes_absent_from_json_defaults_to_zero() {
   973	        let json = r#"{
   974	            "stage": "code",
   975	            "phase": 1,
   976	            "agent": "claude",
   977	            "mode": "auto",
   978	            "started_at": "0",
   979	            "project_root": "/repo"
   980	        }"#;
   981	        let loaded: State = serde_json::from_str(json).unwrap();
   982	        assert_eq!(loaded.checkpoint_resumes, 0);
   983	    }
   984	
   985	    /// 23-09 Task 1: `yes_ship` round-trips through serde as an exact `bool`
   986	    /// — its own key appears in the persisted JSON, and a fresh deserialize
   987	    /// recovers the value set, mirroring the `monitor_pid` pair above.
   988	    #[test]
   989	    fn yes_ship_round_trips_through_serde() {
   990	        let mut state = State::new(
   991	            PhaseId::new(1),
   992	            AgentKind::Claude,
   993	            Mode::Auto,
   994	            PathBuf::from("/repo"),
   995	        );
   996	        state.yes_ship = true;
   997	        let json = serde_json::to_string(&state).unwrap();
   998	        assert!(
   999	            json.contains("yes_ship"),
  1000	            "yes_ship must appear in persisted JSON"
  1001	        );
  1002	        let loaded: State = serde_json::from_str(&json).unwrap();
  1003	        assert!(loaded.yes_ship, "yes_ship must round-trip through serde");
  1004	    }
  1005	
  1006	    /// A serde-absent `yes_ship` (state written by a pre-23-09 binary) must
  1007	    /// deserialize to `false`, not fail to deserialize — the same
  1008	    /// backward-compat pattern as every other `#[serde(default)]` field
  1009	    /// added since 17-01.
  1010	    #[test]
  1011	    fn yes_ship_absent_from_json_defaults_to_false() {
  1012	        let json = r#"{
  1013	            "stage": "code",
  1014	            "phase": 1,
  1015	            "agent": "claude",
  1016	            "mode": "auto",
  1017	            "started_at": "0",
  1018	            "project_root": "/repo"
  1019	        }"#;
  1020	        let loaded: State = serde_json::from_str(json).unwrap();
  1021	        assert!(!loaded.yes_ship);
  1022	    }
  1023	
  1024	    /// 20c: `stop_until`/`stopped`/`stop_reason` all round-trip through
  1025	    /// serde — each field's own key appears in the persisted JSON, and a
  1026	    /// fresh deserialize recovers the exact values set.
  1027	    #[test]
  1028	    fn stop_fields_round_trip_through_serde() {
  1029	        let mut state = State::new(
  1030	            PhaseId::new(1),
  1031	            AgentKind::Claude,
  1032	            Mode::Auto,
  1033	            PathBuf::from("/repo"),
  1034	        );
  1035	        state.stop_until = Some(Stage::Plan);
  1036	        state.stopped = true;
  1037	        state.stop_reason = Some("stopped after plan completed (--until plan)".to_string());
  1038	        let json = serde_json::to_string(&state).unwrap();
  1039	        assert!(
  1040	            json.contains("stop_until") && json.contains("stopped") && json.contains("stop_reason"),
  1041	            "all three stop fields must appear in persisted JSON: {json}"
  1042	        );
  1043	        let loaded: State = serde_json::from_str(&json).unwrap();
  1044	        assert_eq!(
  1045	            loaded.stop_until,
  1046	            Some(Stage::Plan),
  1047	            "stop_until must round-trip through serde"
  1048	        );
  1049	        assert!(loaded.stopped, "stopped must round-trip through serde");
  1050	        assert_eq!(
  1051	            loaded.stop_reason.as_deref(),
  1052	            Some("stopped after plan completed (--until plan)"),
  1053	            "stop_reason must round-trip through serde"
  1054	        );
  1055	    }
  1056	
  1057	    /// A serde-absent `stop_until`/`stopped`/`stop_reason` (state written by
  1058	    /// a pre-20c binary) must default to `None`/`false`/`None`, not fail to
  1059	    /// deserialize — the same backward-compat pattern as every other
  1060	    /// `#[serde(default)]` field added since 17-01.
  1061	    #[test]
  1062	    fn stop_fields_absent_from_json_default() {
  1063	        let json = r#"{
  1064	            "stage": "code",
  1065	            "phase": 1,
  1066	            "agent": "claude",
  1067	            "mode": "auto",
  1068	            "started_at": "0",
  1069	            "project_root": "/repo"
  1070	        }"#;
  1071	        let loaded: State = serde_json::from_str(json).unwrap();
  1072	        assert_eq!(loaded.stop_until, None);
  1073	        assert!(!loaded.stopped);
  1074	        assert_eq!(loaded.stop_reason, None);
  1075	    }
  1076	
  1077	    // ------------------------------------------------------------------
  1078	    // Phase 41 Task 6: AgentKind::Antigravity (unique test prefix so the
  1079	    // verify filter matches ONLY the new work — F6).
  1080	    // ------------------------------------------------------------------
  1081	
  1082	    #[test]
  1083	    fn agent_kind_antigravity_from_str_is_case_insensitive() {
  1084	        assert_eq!(
  1085	            "antigravity".parse::<AgentKind>().unwrap(),
  1086	            AgentKind::Antigravity
  1087	        );
  1088	        assert_eq!(
  1089	            "ANTIGRAVITY".parse::<AgentKind>().unwrap(),
  1090	            AgentKind::Antigravity
  1091	        );
  1092	        assert_eq!(
  1093	            "Antigravity".parse::<AgentKind>().unwrap(),
  1094	            AgentKind::Antigravity
  1095	        );
  1096	    }
  1097	
  1098	    #[test]
  1099	    fn agent_kind_antigravity_display_is_lowercase() {
  1100	        assert_eq!(AgentKind::Antigravity.to_string(), "antigravity");
  1101	    }
  1102	
  1103	    #[test]
  1104	    fn agent_kind_antigravity_serde_round_trips_lowercase() {
  1105	        let wire = serde_json::to_string(&AgentKind::Antigravity).unwrap();
  1106	        assert_eq!(wire, "\"antigravity\"");
  1107	        let back: AgentKind = serde_json::from_str(&wire).unwrap();
  1108	        assert_eq!(back, AgentKind::Antigravity);
  1109	    }
  1110	
  1111	    #[test]
  1112	    fn agent_kind_antigravity_error_message_lists_it() {
  1113	        let err = "aider".parse::<AgentKind>().unwrap_err();
  1114	        assert!(
  1115	            err.to_string().contains("antigravity"),
  1116	            "the parse error must advertise the new agent: {err}"
  1117	        );
  1118	    }
  1119	
  1120	    #[test]
  1121	    fn agent_kind_antigravity_driver_for_resolves() {
  1122	        assert_eq!(
  1123	            crate::agents::driver_for(AgentKind::Antigravity).name(),
  1124	            "Antigravity"
  1125	        );
  1126	    }
  1127	
  1128	    // ------------------------------------------------------------------
  1129	    // Phase 42 Task 2: AgentKind::Hermes (HRMS-01, D-05, D-06).
  1130	    // ------------------------------------------------------------------
  1131	
  1132	    #[test]
  1133	    fn agent_kind_hermes_from_str_is_case_insensitive() {
  1134	        assert_eq!("hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
  1135	        assert_eq!("HERMES".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
  1136	        assert_eq!("Hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
  1137	    }
  1138	
  1139	    #[test]
  1140	    fn agent_kind_hermes_display_is_lowercase() {
  1141	        assert_eq!(AgentKind::Hermes.to_string(), "hermes");
  1142	    }
  1143	
  1144	    #[test]
  1145	    fn agent_kind_hermes_serde_round_trips_lowercase() {
  1146	        let wire = serde_json::to_string(&AgentKind::Hermes).unwrap();
  1147	        assert_eq!(wire, "\"hermes\"");
  1148	        let back: AgentKind = serde_json::from_str(&wire).unwrap();
  1149	        assert_eq!(back, AgentKind::Hermes);
  1150	    }
  1151	
  1152	    #[test]
  1153	    fn agent_kind_hermes_error_message_lists_it() {
  1154	        let err = "aider".parse::<AgentKind>().unwrap_err();
  1155	        assert!(
  1156	            err.to_string().contains("hermes"),
  1157	            "the parse error must advertise the new agent: {err}"
  1158	        );
  1159	    }
  1160	
  1161	    #[test]
  1162	    fn agent_kind_hermes_driver_for_resolves() {
  1163	        assert_eq!(
  1164	            crate::agents::driver_for(AgentKind::Hermes).name(),
  1165	            "Hermes"
  1166	        );
  1167	    }
  1168	}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc 'rg -n "Hermes|hermes|unattended_launch_shape_condition|MonitorReapGuard|test_contract|delegation|stop_until|AgentKind" crates/devflow-cli/src/commands.rs crates/devflow-cli/src/preflight.rs crates/devflow-cli/tests/phase7_cli.rs crates/devflow-core/src/state.rs .planning/phases/42-hermes-driver/*.md' in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 0ms:
crates/devflow-core/src/state.rs:27:/// adds a run-scoped concept (`worktree_path`, `monitor_pid`, `stop_until`,
crates/devflow-core/src/state.rs:41:    pub agent: AgentKind,
crates/devflow-core/src/state.rs:319:    pub stop_until: Option<Stage>,
crates/devflow-core/src/state.rs:320:    /// Set by `transition()` when `stop_until` names the stage just
crates/devflow-core/src/state.rs:377:    /// `devflow resume` would be the same silent-drop class as `stop_until`'s
crates/devflow-core/src/state.rs:387:pub enum AgentKind {
crates/devflow-core/src/state.rs:398:    /// Hermes Agent CLI (`hermes`), headless oneshot (phase 42).
crates/devflow-core/src/state.rs:399:    Hermes,
crates/devflow-core/src/state.rs:402:impl fmt::Display for AgentKind {
crates/devflow-core/src/state.rs:405:            AgentKind::Claude => "claude",
crates/devflow-core/src/state.rs:406:            AgentKind::Codex => "codex",
crates/devflow-core/src/state.rs:407:            AgentKind::OpenCode => "opencode",
crates/devflow-core/src/state.rs:408:            AgentKind::Pi => "pi",
crates/devflow-core/src/state.rs:409:            AgentKind::Antigravity => "antigravity",
crates/devflow-core/src/state.rs:410:            AgentKind::Hermes => "hermes",
crates/devflow-core/src/state.rs:416:impl FromStr for AgentKind {
crates/devflow-core/src/state.rs:421:            "claude" => Ok(AgentKind::Claude),
crates/devflow-core/src/state.rs:422:            "codex" => Ok(AgentKind::Codex),
crates/devflow-core/src/state.rs:423:            "opencode" | "open-code" => Ok(AgentKind::OpenCode),
crates/devflow-core/src/state.rs:424:            "pi" => Ok(AgentKind::Pi),
crates/devflow-core/src/state.rs:425:            "antigravity" => Ok(AgentKind::Antigravity),
crates/devflow-core/src/state.rs:426:            "hermes" => Ok(AgentKind::Hermes),
crates/devflow-core/src/state.rs:434:#[error("unsupported agent `{0}`; expected claude, codex, opencode, pi, antigravity, or hermes")]
crates/devflow-core/src/state.rs:439:    pub fn new(phase: PhaseId, agent: AgentKind, mode: Mode, project_root: PathBuf) -> Self {
crates/devflow-core/src/state.rs:461:            stop_until: None,
crates/devflow-core/src/state.rs:486:        assert_eq!(driver_for(AgentKind::Claude).name(), "Claude Code");
crates/devflow-core/src/state.rs:487:        assert_eq!(driver_for(AgentKind::Codex).name(), "OpenAI Codex");
crates/devflow-core/src/state.rs:488:        assert_eq!(driver_for(AgentKind::OpenCode).name(), "OpenCode");
crates/devflow-core/src/state.rs:490:        assert_eq!(AgentKind::Claude.to_string(), "claude");
crates/devflow-core/src/state.rs:491:        assert_eq!(AgentKind::Codex.to_string(), "codex");
crates/devflow-core/src/state.rs:492:        assert_eq!(AgentKind::OpenCode.to_string(), "opencode");
crates/devflow-core/src/state.rs:497:        assert_eq!("claude".parse::<AgentKind>().unwrap(), AgentKind::Claude);
crates/devflow-core/src/state.rs:498:        assert_eq!("CLAUDE".parse::<AgentKind>().unwrap(), AgentKind::Claude);
crates/devflow-core/src/state.rs:499:        assert_eq!("codex".parse::<AgentKind>().unwrap(), AgentKind::Codex);
crates/devflow-core/src/state.rs:501:            "opencode".parse::<AgentKind>().unwrap(),
crates/devflow-core/src/state.rs:502:            AgentKind::OpenCode
crates/devflow-core/src/state.rs:505:            "open-code".parse::<AgentKind>().unwrap(),
crates/devflow-core/src/state.rs:506:            AgentKind::OpenCode
crates/devflow-core/src/state.rs:512:        let err = "aider".parse::<AgentKind>().unwrap_err();
crates/devflow-core/src/state.rs:520:            AgentKind::Claude,
crates/devflow-core/src/state.rs:526:        assert_eq!(state.agent, AgentKind::Claude);
crates/devflow-core/src/state.rs:535:        assert_eq!(state.stop_until, None);
crates/devflow-core/src/state.rs:545:            AgentKind::Codex,
crates/devflow-core/src/state.rs:552:        assert_eq!(back.agent, AgentKind::Codex);
crates/devflow-core/src/state.rs:561:            AgentKind::Claude,
crates/devflow-core/src/state.rs:584:            AgentKind::Claude,
crates/devflow-core/src/state.rs:627:            AgentKind::Claude,
crates/devflow-core/src/state.rs:672:            AgentKind::Claude,
crates/devflow-core/src/state.rs:717:            AgentKind::Claude,
crates/devflow-core/src/state.rs:769:            AgentKind::Claude,
crates/devflow-core/src/state.rs:792:            AgentKind::Claude,
crates/devflow-core/src/state.rs:838:            AgentKind::Claude,
crates/devflow-core/src/state.rs:871:            AgentKind::Claude,
crates/devflow-core/src/state.rs:912:            AgentKind::Claude,
crates/devflow-core/src/state.rs:952:            AgentKind::Claude,
crates/devflow-core/src/state.rs:992:            AgentKind::Claude,
crates/devflow-core/src/state.rs:1024:    /// 20c: `stop_until`/`stopped`/`stop_reason` all round-trip through
crates/devflow-core/src/state.rs:1031:            AgentKind::Claude,
crates/devflow-core/src/state.rs:1035:        state.stop_until = Some(Stage::Plan);
crates/devflow-core/src/state.rs:1040:            json.contains("stop_until") && json.contains("stopped") && json.contains("stop_reason"),
crates/devflow-core/src/state.rs:1045:            loaded.stop_until,
crates/devflow-core/src/state.rs:1047:            "stop_until must round-trip through serde"
crates/devflow-core/src/state.rs:1057:    /// A serde-absent `stop_until`/`stopped`/`stop_reason` (state written by
crates/devflow-core/src/state.rs:1072:        assert_eq!(loaded.stop_until, None);
crates/devflow-core/src/state.rs:1078:    // Phase 41 Task 6: AgentKind::Antigravity (unique test prefix so the
crates/devflow-core/src/state.rs:1085:            "antigravity".parse::<AgentKind>().unwrap(),
crates/devflow-core/src/state.rs:1086:            AgentKind::Antigravity
crates/devflow-core/src/state.rs:1089:            "ANTIGRAVITY".parse::<AgentKind>().unwrap(),
crates/devflow-core/src/state.rs:1090:            AgentKind::Antigravity
crates/devflow-core/src/state.rs:1093:            "Antigravity".parse::<AgentKind>().unwrap(),
crates/devflow-core/src/state.rs:1094:            AgentKind::Antigravity
crates/devflow-core/src/state.rs:1100:        assert_eq!(AgentKind::Antigravity.to_string(), "antigravity");
crates/devflow-core/src/state.rs:1105:        let wire = serde_json::to_string(&AgentKind::Antigravity).unwrap();
crates/devflow-core/src/state.rs:1107:        let back: AgentKind = serde_json::from_str(&wire).unwrap();
crates/devflow-core/src/state.rs:1108:        assert_eq!(back, AgentKind::Antigravity);
crates/devflow-core/src/state.rs:1113:        let err = "aider".parse::<AgentKind>().unwrap_err();
crates/devflow-core/src/state.rs:1123:            crate::agents::driver_for(AgentKind::Antigravity).name(),
crates/devflow-core/src/state.rs:1129:    // Phase 42 Task 2: AgentKind::Hermes (HRMS-01, D-05, D-06).
crates/devflow-core/src/state.rs:1133:    fn agent_kind_hermes_from_str_is_case_insensitive() {
crates/devflow-core/src/state.rs:1134:        assert_eq!("hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
crates/devflow-core/src/state.rs:1135:        assert_eq!("HERMES".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
crates/devflow-core/src/state.rs:1136:        assert_eq!("Hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
crates/devflow-core/src/state.rs:1140:    fn agent_kind_hermes_display_is_lowercase() {
crates/devflow-core/src/state.rs:1141:        assert_eq!(AgentKind::Hermes.to_string(), "hermes");
crates/devflow-core/src/state.rs:1145:    fn agent_kind_hermes_serde_round_trips_lowercase() {
crates/devflow-core/src/state.rs:1146:        let wire = serde_json::to_string(&AgentKind::Hermes).unwrap();
crates/devflow-core/src/state.rs:1147:        assert_eq!(wire, "\"hermes\"");
crates/devflow-core/src/state.rs:1148:        let back: AgentKind = serde_json::from_str(&wire).unwrap();
crates/devflow-core/src/state.rs:1149:        assert_eq!(back, AgentKind::Hermes);
crates/devflow-core/src/state.rs:1153:    fn agent_kind_hermes_error_message_lists_it() {
crates/devflow-core/src/state.rs:1154:        let err = "aider".parse::<AgentKind>().unwrap_err();
crates/devflow-core/src/state.rs:1156:            err.to_string().contains("hermes"),
crates/devflow-core/src/state.rs:1162:    fn agent_kind_hermes_driver_for_resolves() {
crates/devflow-core/src/state.rs:1164:            crate::agents::driver_for(AgentKind::Hermes).name(),
crates/devflow-core/src/state.rs:1165:            "Hermes"
crates/devflow-cli/src/preflight.rs:33:use devflow_core::state::{AgentKind, State};
crates/devflow-cli/src/preflight.rs:84:pub(crate) fn agent_program(agent: AgentKind) -> &'static str {
crates/devflow-cli/src/preflight.rs:980:/// replacing `state.agent == AgentKind::Claude` with an explicit dogfood flag.
crates/devflow-cli/src/preflight.rs:981:fn unattended_launch_shape_condition(state: &State) -> ConditionState {
crates/devflow-cli/src/preflight.rs:983:        && (state.agent == AgentKind::Claude || state.agent == AgentKind::Antigravity)
crates/devflow-cli/src/preflight.rs:991:    if state.agent != AgentKind::Claude && state.agent != AgentKind::Antigravity {
crates/devflow-cli/src/preflight.rs:1125:            unattended_launch_shape_condition(state),
crates/devflow-cli/src/preflight.rs:1393:            AgentKind::Codex,
crates/devflow-cli/src/preflight.rs:1408:        state.agent = AgentKind::Claude;
crates/devflow-cli/src/preflight.rs:1413:        state.agent = AgentKind::Codex;
crates/devflow-cli/src/preflight.rs:1494:            AgentKind::Claude,
crates/devflow-cli/src/preflight.rs:1516:            AgentKind::Claude,
crates/devflow-cli/src/preflight.rs:1537:            AgentKind::Claude,
crates/devflow-cli/src/preflight.rs:1575:            AgentKind::Claude,
crates/devflow-cli/src/preflight.rs:1665:            AgentKind::Claude,
crates/devflow-cli/src/preflight.rs:1716:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:1729:        let adapter = agents::driver_for(AgentKind::Claude);
crates/devflow-cli/src/preflight.rs:1782:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:1787:        let adapter = agents::driver_for(AgentKind::Claude);
crates/devflow-cli/src/preflight.rs:1847:            AgentKind::Claude,
crates/devflow-cli/src/preflight.rs:1889:        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:1901:        let adapter = agents::driver_for(AgentKind::Codex);
crates/devflow-cli/src/preflight.rs:1928:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:1973:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:2070:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:2202:        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:2292:        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:2357:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:2362:        let adapter = agents::driver_for(AgentKind::Claude);
crates/devflow-cli/src/preflight.rs:3312:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root);
crates/devflow-cli/src/preflight.rs:3335:            AgentKind::Codex,
crates/devflow-cli/src/preflight.rs:3471:        for agent in [AgentKind::Codex, AgentKind::OpenCode] {
crates/devflow-cli/src/preflight.rs:3480:        state.agent = AgentKind::Claude;
crates/devflow-cli/src/preflight.rs:3617:        let mut state = State::new(phase, AgentKind::Codex, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/preflight.rs:3661:    fn unattended_launch_shape_condition_antigravity_allowed() {
crates/devflow-cli/src/preflight.rs:3665:            AgentKind::Antigravity,
crates/devflow-cli/src/preflight.rs:3670:        let condition = unattended_launch_shape_condition(&state);
crates/devflow-cli/src/preflight.rs:3678:    fn unattended_launch_shape_condition_claude_unchanged() {
crates/devflow-cli/src/preflight.rs:3682:            AgentKind::Claude,
crates/devflow-cli/src/preflight.rs:3690:                unattended_launch_shape_condition(&state),
crates/devflow-cli/src/preflight.rs:3698:        let ConditionState::DoesNotHold(reason) = unattended_launch_shape_condition(&state) else {
crates/devflow-cli/src/preflight.rs:3708:    fn unattended_launch_shape_condition_non_stream_agent_refused() {
crates/devflow-cli/src/preflight.rs:3712:            AgentKind::Pi,
crates/devflow-cli/src/preflight.rs:3717:        let ConditionState::DoesNotHold(reason) = unattended_launch_shape_condition(&state) else {
crates/devflow-cli/src/preflight.rs:3725:            agent_program(AgentKind::Antigravity),
.planning/phases/42-hermes-driver/42-02-PLAN.md:2:phase: 42-hermes-driver
.planning/phases/42-hermes-driver/42-02-PLAN.md:9:  - .planning/phases/42-hermes-driver/42-VERIFICATION.md
.planning/phases/42-hermes-driver/42-02-PLAN.md:10:  - .planning/phases/42-hermes-driver/42-UAT.md
.planning/phases/42-hermes-driver/42-02-PLAN.md:27:    - `crates/devflow-cli/src/preflight.rs` — updated `unattended_launch_shape_condition` allowing Antigravity in unattended mode
.planning/phases/42-hermes-driver/42-02-PLAN.md:28:    - `.planning/phases/42-hermes-driver/42-VERIFICATION.md` — dogfood execution evidence and cadence measurement log
.planning/phases/42-hermes-driver/42-02-PLAN.md:29:    - `.planning/phases/42-hermes-driver/42-UAT.md` — UAT sign-off including cadence verification and 60m print-timeout confirmation
.planning/phases/42-hermes-driver/42-02-PLAN.md:48:@.planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-02-PLAN.md:49:@.planning/phases/42-hermes-driver/42-RESEARCH.md
.planning/phases/42-hermes-driver/42-02-PLAN.md:60:  <files>.planning/phases/42-hermes-driver/42-CONTEXT.md</files>
.planning/phases/42-hermes-driver/42-02-PLAN.md:63:    .planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-02-PLAN.md:67:    - `hermes --version` exits 0 and reports Hermes Agent
.planning/phases/42-hermes-driver/42-02-PLAN.md:68:    - `devflow doctor` reports both Antigravity and Hermes present
.planning/phases/42-hermes-driver/42-02-PLAN.md:69:    - `cargo test -p devflow-core --lib hermes` exits 0
.planning/phases/42-hermes-driver/42-02-PLAN.md:74:    2. Verify `hermes` binary is on PATH.
.planning/phases/42-hermes-driver/42-02-PLAN.md:79:    <automated>agy --version && hermes --version && echo "preconditions verified"</automated>
.planning/phases/42-hermes-driver/42-02-PLAN.md:86:  <files>.planning/phases/42-hermes-driver/42-VERIFICATION.md, .planning/phases/42-hermes-driver/42-UAT.md</files>
.planning/phases/42-hermes-driver/42-02-PLAN.md:90:    .planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-02-PLAN.md:97:    - Evidence recorded in `.planning/phases/42-hermes-driver/42-VERIFICATION.md` and `42-UAT.md`
.planning/phases/42-hermes-driver/42-02-PLAN.md:118:    .planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-02-PLAN.md:121:    - `unattended_launch_shape_condition` in `crates/devflow-cli/src/preflight.rs` permits `AgentKind::Antigravity` in unattended mode (`--mode auto`) following successful dogfooding (ANTG-04, D-07)
.planning/phases/42-hermes-driver/42-02-PLAN.md:122:    - Unit tests in `preflight.rs` updated: `unattended_launch_shape_condition_antigravity_allowed` asserts `ConditionState::Holds` for Antigravity
.planning/phases/42-hermes-driver/42-02-PLAN.md:123:    - `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity` exits 0
.planning/phases/42-hermes-driver/42-02-PLAN.md:127:    1. Update `unattended_launch_shape_condition(state: &State)` to include `AgentKind::Antigravity` alongside `AgentKind::Claude` when `stream_launch_enabled` holds and `!state.legacy_claude_launch`.
.planning/phases/42-hermes-driver/42-02-PLAN.md:131:    <automated>cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity -- --nocapture</automated>
.planning/phases/42-hermes-driver/42-02-PLAN.md:156:2. `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity` passes.
.planning/phases/42-hermes-driver/42-02-PLAN.md:165:Create `.planning/phases/42-hermes-driver/42-02-SUMMARY.md` when done
.planning/phases/42-hermes-driver/42-02-PLAN.md:169:- `unattended_launch_shape_condition_antigravity_allowed` (test function in `crates/devflow-cli/src/preflight.rs`)
.planning/phases/42-hermes-driver/42-02-PLAN.md:170:- `42-VERIFICATION.md` (evidence artifact in `.planning/phases/42-hermes-driver/42-VERIFICATION.md`)
.planning/phases/42-hermes-driver/42-02-PLAN.md:171:- `42-UAT.md` (UAT artifact in `.planning/phases/42-hermes-driver/42-UAT.md`)
crates/devflow-cli/tests/phase7_cli.rs:160:/// Suite-level registry of monitor PIDs the tests' `MonitorReapGuard`s are
crates/devflow-cli/tests/phase7_cli.rs:163:/// Populated by [`MonitorReapGuard::after_launch`] (the pid the guard will
crates/devflow-cli/tests/phase7_cli.rs:179:/// Count of bound-but-not-yet-dropped `MonitorReapGuard`s — the suite audit's
crates/devflow-cli/tests/phase7_cli.rs:379:    let _reap7 = MonitorReapGuard::after_launch(&state7);
crates/devflow-cli/tests/phase7_cli.rs:380:    let _reap8 = MonitorReapGuard::after_launch(&state8);
crates/devflow-cli/tests/phase7_cli.rs:431:    let _reap = MonitorReapGuard::after_launch(&settled);
crates/devflow-cli/tests/phase7_cli.rs:498:    let _reap = MonitorReapGuard::after_launch(&settled);
crates/devflow-cli/tests/phase7_cli.rs:546:    let _reap = MonitorReapGuard::after_launch(&settled);
crates/devflow-cli/tests/phase7_cli.rs:554:/// `state.stage == Plan`, which is exactly the `stop_until == Some(from)`
crates/devflow-cli/tests/phase7_cli.rs:609:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:788:        "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
crates/devflow-cli/tests/phase7_cli.rs:890:    let _reap = MonitorReapGuard::after_launch(&settled);
crates/devflow-cli/tests/phase7_cli.rs:992:        devflow_core::state::AgentKind::Claude,
crates/devflow-cli/tests/phase7_cli.rs:1056:        devflow_core::state::AgentKind::Claude,
crates/devflow-cli/tests/phase7_cli.rs:1121:        devflow_core::state::AgentKind::Claude,
crates/devflow-cli/tests/phase7_cli.rs:1178:        devflow_core::state::AgentKind::Claude,
crates/devflow-cli/tests/phase7_cli.rs:1227:        devflow_core::state::AgentKind::Claude,
crates/devflow-cli/tests/phase7_cli.rs:1288:/// `MonitorReapGuard` must capture (a guard bound to an early state read
crates/devflow-cli/tests/phase7_cli.rs:1347:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:1380:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:1443:    let _reap = MonitorReapGuard::after_launch(&gated);
crates/devflow-cli/tests/phase7_cli.rs:1517:struct MonitorReapGuard {
crates/devflow-cli/tests/phase7_cli.rs:1521:impl MonitorReapGuard {
crates/devflow-cli/tests/phase7_cli.rs:1538:impl Drop for MonitorReapGuard {
crates/devflow-cli/tests/phase7_cli.rs:1561:                    "MonitorReapGuard: monitor wrapper pid {pid} still alive after reap \
crates/devflow-cli/tests/phase7_cli.rs:1600:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:1638:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:1675:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:1740:/// pid by hand — exactly what [`MonitorReapGuard::after_launch`] would have
crates/devflow-cli/tests/phase7_cli.rs:1786:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:1790:// Phase 42 Task 4 (HRMS-03, D-03): Hermes transport integration tests.
crates/devflow-cli/tests/phase7_cli.rs:1793:fn hermes_stub(launch: &str) -> String {
crates/devflow-cli/tests/phase7_cli.rs:1797:    printf 'hermes 0.5.0\n'
crates/devflow-cli/tests/phase7_cli.rs:1801:    printf 'Available Toolsets:\n  ✓ enabled delegation 👥 Task Delegation\n  ✓ enabled terminal 💻 Terminal Execution\n'
crates/devflow-cli/tests/phase7_cli.rs:1809:/// HRMS-03: a stubbed `hermes` that exits 0 with no marker must not advance a
crates/devflow-cli/tests/phase7_cli.rs:1812:fn hermes_marker_less_run_does_not_advance() {
crates/devflow-cli/tests/phase7_cli.rs:1816:    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("printf 'fake hermes, no marker\\n'\nexit 0\n"))]);
crates/devflow-cli/tests/phase7_cli.rs:1826:            "hermes",
crates/devflow-cli/tests/phase7_cli.rs:1836:        "a marker-less hermes run must not advance past the commit-gated Plan stage"
crates/devflow-cli/tests/phase7_cli.rs:1839:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:1842:/// HRMS-03: a Hermes run that exits non-zero must not advance its stage.
crates/devflow-cli/tests/phase7_cli.rs:1844:fn hermes_nonzero_exit_does_not_advance() {
crates/devflow-cli/tests/phase7_cli.rs:1848:    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("printf 'fake hermes error\\n'\nexit 1\n"))]);
crates/devflow-cli/tests/phase7_cli.rs:1858:            "hermes",
crates/devflow-cli/tests/phase7_cli.rs:1868:        "a non-zero-exit hermes run must not advance its stage"
crates/devflow-cli/tests/phase7_cli.rs:1871:    let _reap = MonitorReapGuard::after_launch(&state);
crates/devflow-cli/tests/phase7_cli.rs:1874:/// HRMS-03: a hung Hermes process is detected as alive by monitor liveness,
crates/devflow-cli/tests/phase7_cli.rs:1877:fn hermes_hung_process_is_detected_not_left_running() {
crates/devflow-cli/tests/phase7_cli.rs:1881:    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("exec sleep 30\n"))]);
crates/devflow-cli/tests/phase7_cli.rs:1891:            "hermes",
crates/devflow-cli/tests/phase7_cli.rs:1907:        "the hung hermes process must still be alive"
crates/devflow-cli/tests/phase7_cli.rs:1913:        "the stage must not advance while hermes is hung"
crates/devflow-cli/tests/phase7_cli.rs:1922:        "should be able to kill the hung hermes process"
crates/devflow-cli/tests/phase7_cli.rs:1928:        "a hung-then-killed hermes run must not advance its stage"
crates/devflow-cli/tests/phase7_cli.rs:1930:    let _reap = MonitorReapGuard::after_launch(&gated);
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:1:# Phase 42: Hermes Driver - Discussion Log
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:7:**Phase:** 42-hermes-driver
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:8:**Areas discussed:** Hermes CLI argv & flags, Prompt rendering style, Subagent dispatch capabilities, Antigravity dogfooding execution
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:12:## Hermes CLI Argv & Flags
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:16:| Bare oneshot | `hermes -z "<prompt>" --yolo` | |
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:17:| Headless-safe with hooks auto-accept | `hermes -z "<prompt>" --yolo --accept-hooks` + `HERMES_ACCEPT_HOOKS=1` | ✓ |
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:19:**User's choice:** Use `hermes -z "<prompt>" --yolo --accept-hooks` + `HERMES_ACCEPT_HOOKS=1` (headless-safe)
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:29:| `render_workflow_style` | Workflow file paths (`$HOME/.hermes/...`) + `DEVFLOW_RESULT` marker | |
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:40:| Dynamic Probe | Check `hermes tools list` for `enabled.*delegation` | ✓ |
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:43:**User's choice:** Dynamic probe — check `hermes tools list` for enabled delegation toolset.
.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:44:**Notes:** Hermes CLI includes a built-in `delegation` toolset (`✓ enabled delegation 👥 Task Delegation`). Implemented dynamic probe helper mirroring `pi_subagent_dispatch_available`.
crates/devflow-cli/src/commands.rs:39:use devflow_core::state::{AgentKind, State};
crates/devflow-cli/src/commands.rs:150:    agent: AgentKind,
crates/devflow-cli/src/commands.rs:201:    agent: AgentKind,
crates/devflow-cli/src/commands.rs:214:    state.stop_until = until;
crates/devflow-cli/src/commands.rs:983:///    `state.stop_until == Some(from)` arm near the top of `transition`),
crates/devflow-cli/src/commands.rs:1744:/// than overwriting. Never touches `stop_until` — that field means
crates/devflow-cli/src/commands.rs:1975:        "Cron instruction pending (phase {}): hermes cron create --from-devflow {}",
crates/devflow-cli/src/commands.rs:2339:        // Phase 42 Task 3 (HRMS-01, D-06): presence-only probe of the `hermes` binary.
crates/devflow-cli/src/commands.rs:2341:            "hermes",
crates/devflow-cli/src/commands.rs:2342:            "hermes",
crates/devflow-cli/src/commands.rs:2344:            "Install the Hermes Agent CLI so `hermes` is on PATH",
crates/devflow-cli/src/commands.rs:2472:    let dispatch = agents::driver_for(AgentKind::Pi)
crates/devflow-cli/src/commands.rs:3798:        let mut persisted = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:3806:        persisted.stop_until = Some(Stage::Code);
crates/devflow-cli/src/commands.rs:3809:        let fresh = fresh_state_carrying_phase_failures(root, phase, AgentKind::Claude, Mode::Auto);
crates/devflow-cli/src/commands.rs:3829:        assert_eq!(fresh.stop_until, None);
crates/devflow-cli/src/commands.rs:3846:        let mut persisted = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:3850:            fresh_state_carrying_phase_failures(root, phase, AgentKind::Claude, Mode::Auto)
crates/devflow-cli/src/commands.rs:3861:            fresh_state_carrying_phase_failures(root, phase, AgentKind::Claude, Mode::Auto)
crates/devflow-cli/src/commands.rs:3895:        let mut persisted = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:4083:    fn cron_instruction_hints_include_hermes_command_per_phase() {
crates/devflow-cli/src/commands.rs:4086:        // the base hermes-command hint from 21a's reset-time fragment
crates/devflow-cli/src/commands.rs:4100:                "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
crates/devflow-cli/src/commands.rs:4119:            "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
crates/devflow-cli/src/commands.rs:4139:                "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
crates/devflow-cli/src/commands.rs:4151:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:4166:                AgentKind::Claude,
crates/devflow-cli/src/commands.rs:4248:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:4257:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:4281:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:5016:        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:5030:        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:5169:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:5286:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:5301:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:5323:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:5365:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:5701:            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:5733:            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:5793:            let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
crates/devflow-cli/src/commands.rs:6111:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:6126:            AgentKind::Claude,
crates/devflow-cli/src/commands.rs:6248:        let mut state = State::new(PhaseId::new(1), AgentKind::Claude, Mode::Auto, root.clone());
crates/devflow-cli/src/commands.rs:6840:    fn doctor_includes_hermes_check_in_the_seam() {
crates/devflow-cli/src/commands.rs:6842:        let hermes = checks
crates/devflow-cli/src/commands.rs:6844:            .find(|c| c.name == "hermes")
crates/devflow-cli/src/commands.rs:6845:            .expect("doctor_checks() must contain the hermes entry");
crates/devflow-cli/src/commands.rs:6846:        if hermes.status == "missing" {
crates/devflow-cli/src/commands.rs:6848:                hermes.install_hint.as_deref().unwrap_or("").contains("hermes"),
crates/devflow-cli/src/commands.rs:6849:                "the hint must name the hermes binary: {:?}",
crates/devflow-cli/src/commands.rs:6850:                hermes.install_hint
.planning/phases/42-hermes-driver/42-01-PLAN.md:2:phase: 42-hermes-driver
.planning/phases/42-hermes-driver/42-01-PLAN.md:8:  - crates/devflow-core/src/agents/hermes.rs
.planning/phases/42-hermes-driver/42-01-PLAN.md:24:    - "`--agent hermes` resolves through `AgentKind`/`driver_for`/`agent_program`, and `devflow doctor` reports it installed (HRMS-01, D-05, D-06)"
.planning/phases/42-hermes-driver/42-01-PLAN.md:25:    - "`HermesDriver::build_command` returns `(\"hermes\", vec![\"-z\".into(), prompt.to_string(), \"--yolo\".into(), \"--accept-hooks\".into()])` and `environment()` sets `HERMES_ACCEPT_HOOKS=1` (HRMS-02, D-01)"
.planning/phases/42-hermes-driver/42-01-PLAN.md:26:    - "`HermesDriver::render_prompt` delegates to `crate::prompt::render_claude_style(intent)` (D-02)"
.planning/phases/42-hermes-driver/42-01-PLAN.md:27:    - "`HermesDriver::capabilities` probes `hermes tools list` for enabled delegation via `hermes_subagent_dispatch_available()` (D-04)"
.planning/phases/42-hermes-driver/42-01-PLAN.md:28:    - "HermesDriver is enrolled in the shared conformance suite (5 → 6 drivers), proven by the `hermes_conformance_enrollment` unit test (D-06)"
.planning/phases/42-hermes-driver/42-01-PLAN.md:29:    - "Completion is process-exit + `DEVFLOW_RESULT` prompt contract; a stubbed `hermes` binary with exit 0 and no marker does NOT advance a commit-gated stage (HRMS-03, D-03)"
.planning/phases/42-hermes-driver/42-01-PLAN.md:30:    - "Non-zero exit codes and hung `hermes` processes are handled cleanly without advancing stages or leaking processes (HRMS-03)"
.planning/phases/42-hermes-driver/42-01-PLAN.md:32:    - `crates/devflow-core/src/agents/hermes.rs` — HermesDriver implementation, environment, subagent capability probe, and unit tests
.planning/phases/42-hermes-driver/42-01-PLAN.md:33:    - `crates/devflow-core/src/state.rs` — `AgentKind::Hermes` variant, `Display`, `FromStr`, and serde serialization
.planning/phases/42-hermes-driver/42-01-PLAN.md:35:    - `crates/devflow-cli/src/commands.rs` — `doctor_checks()` hermes presence check
.planning/phases/42-hermes-driver/42-01-PLAN.md:36:    - `crates/devflow-cli/tests/phase7_cli.rs` — hermes stub fixture, marker-less regression, non-zero exit regression, and hung process detection
.planning/phases/42-hermes-driver/42-01-PLAN.md:40:Implement the modular Hermes agent driver (`AgentKind::Hermes`), register it across `devflow-core` and `devflow-cli`, enroll it in the shared conformance test suite, add presence-only health checks to `devflow doctor`, and verify process-exit failure modes (marker-less, non-zero exit, hung process) via integration tests.
.planning/phases/42-hermes-driver/42-01-PLAN.md:42:Purpose: Deliver full Hermes agent support in DevFlow (HRMS-01, HRMS-02, HRMS-03) matching the modular driver contract.
.planning/phases/42-hermes-driver/42-01-PLAN.md:43:Output: Working `HermesDriver` module, updated `state.rs`, `agents/mod.rs`, `commands.rs`, and green unit/integration tests.
.planning/phases/42-hermes-driver/42-01-PLAN.md:55:@.planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-01-PLAN.md:56:@.planning/phases/42-hermes-driver/42-RESEARCH.md
.planning/phases/42-hermes-driver/42-01-PLAN.md:57:@.planning/phases/42-hermes-driver/42-PATTERNS.md
.planning/phases/42-hermes-driver/42-01-PLAN.md:69:  <name>Task 1: Implement HermesDriver in crates/devflow-core/src/agents/hermes.rs</name>
.planning/phases/42-hermes-driver/42-01-PLAN.md:70:  <files>crates/devflow-core/src/agents/hermes.rs</files>
.planning/phases/42-hermes-driver/42-01-PLAN.md:76:    .planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-01-PLAN.md:79:    - `crates/devflow-core/src/agents/hermes.rs` exists and defines `pub struct HermesDriver` implementing `AgentDriver`
.planning/phases/42-hermes-driver/42-01-PLAN.md:80:    - `HermesDriver::name()` returns `"Hermes"`
.planning/phases/42-hermes-driver/42-01-PLAN.md:81:    - `HermesDriver::render_prompt()` delegates to `crate::prompt::render_claude_style(intent)` (D-02)
.planning/phases/42-hermes-driver/42-01-PLAN.md:82:    - `HermesDriver::build_command()` returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])` (D-01)
.planning/phases/42-hermes-driver/42-01-PLAN.md:83:    - `HermesDriver::environment()` returns `vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]` (D-01)
.planning/phases/42-hermes-driver/42-01-PLAN.md:84:    - `HermesDriver::capabilities()` returns `DriverCapabilities { subagent_dispatch: hermes_subagent_dispatch_available() }` (D-04)
.planning/phases/42-hermes-driver/42-01-PLAN.md:85:    - `hermes_subagent_dispatch_available()` runs `hermes tools list` and checks for `enabled` and `delegation`
.planning/phases/42-hermes-driver/42-01-PLAN.md:86:    - Unit tests in `hermes.rs` verify `exec_command_shape`, `environment_contains_accept_hooks`, `render_prompt_claude_style`, and `capabilities_detect_delegation`
.planning/phases/42-hermes-driver/42-01-PLAN.md:87:    - `cargo test -p devflow-core --lib hermes_driver` exits 0
.planning/phases/42-hermes-driver/42-01-PLAN.md:90:    Create `crates/devflow-core/src/agents/hermes.rs` implementing `AgentDriver` for `HermesDriver`.
.planning/phases/42-hermes-driver/42-01-PLAN.md:91:    Implement `name`, `render_prompt` (reusing `render_claude_style` per D-02), `build_command` (`-z`, prompt, `--yolo`, `--accept-hooks` per D-01), `environment` (`HERMES_ACCEPT_HOOKS=1` per D-01), `capabilities` (using `hermes_subagent_dispatch_available` per D-04), and `health` (presence-only).
.planning/phases/42-hermes-driver/42-01-PLAN.md:92:    Include unit tests with stubbed PATH tests verifying command shape, environment, and dynamic delegation detection.
.planning/phases/42-hermes-driver/42-01-PLAN.md:95:    <automated>cargo test -p devflow-core --lib hermes_driver -- --nocapture</automated>
.planning/phases/42-hermes-driver/42-01-PLAN.md:97:  <done>HermesDriver is implemented with unit tests and verified green.</done>
.planning/phases/42-hermes-driver/42-01-PLAN.md:101:  <name>Task 2: Register AgentKind::Hermes and wire driver dispatch and conformance suite</name>
.planning/phases/42-hermes-driver/42-01-PLAN.md:106:    .planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-01-PLAN.md:109:    - `crates/devflow-core/src/state.rs` enum `AgentKind` includes `Hermes` variant (D-05)
.planning/phases/42-hermes-driver/42-01-PLAN.md:110:    - `AgentKind::Hermes.to_string()` returns `"hermes"`
.planning/phases/42-hermes-driver/42-01-PLAN.md:111:    - `"hermes".parse::<AgentKind>()` returns `Ok(AgentKind::Hermes)`
.planning/phases/42-hermes-driver/42-01-PLAN.md:112:    - `crates/devflow-core/src/agents/mod.rs` exposes `pub mod hermes;` and `pub use hermes::HermesDriver;`
.planning/phases/42-hermes-driver/42-01-PLAN.md:113:    - `driver_for(AgentKind::Hermes)` returns `Box::new(HermesDriver)`
.planning/phases/42-hermes-driver/42-01-PLAN.md:115:    - Test `hermes_conformance_enrollment` in `agents/mod.rs` tests `HermesDriver` against all 7 contract checks
.planning/phases/42-hermes-driver/42-01-PLAN.md:116:    - `cargo test -p devflow-core --lib agent_kind_hermes` exits 0
.planning/phases/42-hermes-driver/42-01-PLAN.md:117:    - `cargo test -p devflow-core --lib hermes_conformance_enrollment` exits 0
.planning/phases/42-hermes-driver/42-01-PLAN.md:120:    1. In `crates/devflow-core/src/state.rs`: Add `Hermes` variant to `AgentKind`, update `Display`, `FromStr`, `AgentParseError` message, and add unit tests for `AgentKind::Hermes` serialization and parsing.
.planning/phases/42-hermes-driver/42-01-PLAN.md:121:    2. In `crates/devflow-core/src/agents/mod.rs`: Add `pub mod hermes;`, `pub use hermes::HermesDriver;`, wire `driver_for(AgentKind::Hermes) => Box::new(HermesDriver)`, update `every_driver_passes_the_conformance_suite` to include `Box::new(HermesDriver)` in its 6-element array, and add `hermes_conformance_enrollment` test.
.planning/phases/42-hermes-driver/42-01-PLAN.md:124:    <automated>cargo test -p devflow-core --lib agent_kind_hermes -- --nocapture && cargo test -p devflow-core --lib hermes_conformance_enrollment -- --nocapture</automated>
.planning/phases/42-hermes-driver/42-01-PLAN.md:126:  <done>AgentKind::Hermes is registered, driver_for is wired, and conformance enrollment is verified.</done>
.planning/phases/42-hermes-driver/42-01-PLAN.md:130:  <name>Task 3: Add Hermes presence probe to devflow doctor</name>
.planning/phases/42-hermes-driver/42-01-PLAN.md:134:    .planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-01-PLAN.md:137:    - `doctor_checks()` in `crates/devflow-cli/src/commands.rs` includes a `cmd_check` for `"hermes"` checking `hermes --version` (D-06)
.planning/phases/42-hermes-driver/42-01-PLAN.md:138:    - Unit test `doctor_includes_hermes` in `commands.rs` verifies that the `doctor_checks()` list contains the `"hermes"` check
.planning/phases/42-hermes-driver/42-01-PLAN.md:139:    - `cargo test -p devflow --bin devflow doctor_includes_hermes` exits 0
.planning/phases/42-hermes-driver/42-01-PLAN.md:142:    Add `"hermes"` `cmd_check` entry to `doctor_checks()` in `crates/devflow-cli/src/commands.rs`.
.planning/phases/42-hermes-driver/42-01-PLAN.md:143:    Add unit test `doctor_includes_hermes` verifying the check is present in `doctor_checks()`.
.planning/phases/42-hermes-driver/42-01-PLAN.md:146:    <automated>cargo test -p devflow --bin devflow doctor_includes_hermes -- --nocapture</automated>
.planning/phases/42-hermes-driver/42-01-PLAN.md:148:  <done>devflow doctor includes Hermes presence check, verified by unit test.</done>
.planning/phases/42-hermes-driver/42-01-PLAN.md:152:  <name>Task 4: Add Hermes integration regression tests in phase7_cli.rs</name>
.planning/phases/42-hermes-driver/42-01-PLAN.md:156:    .planning/phases/42-hermes-driver/42-CONTEXT.md
.planning/phases/42-hermes-driver/42-01-PLAN.md:159:    - `crates/devflow-cli/tests/phase7_cli.rs` defines `hermes_stub(launch: &str)`
.planning/phases/42-hermes-driver/42-01-PLAN.md:160:    - Test `hermes_marker_less_run_does_not_advance` proves exit 0 without DEVFLOW_RESULT does not advance commit-gated stage (HRMS-03, D-03)
.planning/phases/42-hermes-driver/42-01-PLAN.md:161:    - Test `hermes_nonzero_exit_does_not_advance` proves exit 1 halts/gates the pipeline (HRMS-03)
.planning/phases/42-hermes-driver/42-01-PLAN.md:162:    - Test `hermes_hung_process_is_detected_not_left_running` proves a hung hermes process is detected and reaped (HRMS-03)
.planning/phases/42-hermes-driver/42-01-PLAN.md:163:    - All tests bind `MonitorReapGuard` and clean up processes cleanly
.planning/phases/42-hermes-driver/42-01-PLAN.md:164:    - `cargo test -p devflow --test phase7_cli hermes` exits 0
.planning/phases/42-hermes-driver/42-01-PLAN.md:168:    1. Define `hermes_stub(launch: &str) -> String` that handles `tools list` (`✓ enabled delegation 👥 Task Delegation`) and `launch` for stage runs.
.planning/phases/42-hermes-driver/42-01-PLAN.md:170:       - `hermes_marker_less_run_does_not_advance()`: stubbed hermes exits 0 with no marker; asserts stage does not advance past Plan.
.planning/phases/42-hermes-driver/42-01-PLAN.md:171:       - `hermes_nonzero_exit_does_not_advance()`: stubbed hermes exits 1; asserts stage does not advance.
.planning/phases/42-hermes-driver/42-01-PLAN.md:172:       - `hermes_hung_process_is_detected_not_left_running()`: stubbed hermes sleeps; asserts liveness detection and process cleanup.
.planning/phases/42-hermes-driver/42-01-PLAN.md:175:    <automated>cargo test -p devflow --test phase7_cli hermes -- --nocapture</automated>
.planning/phases/42-hermes-driver/42-01-PLAN.md:177:  <done>Hermes transport integration tests are implemented and verified green.</done>
.planning/phases/42-hermes-driver/42-01-PLAN.md:187:| DevFlow process monitor → Hermes CLI | Spawning `hermes -z` with `--yolo` and `--accept-hooks` executes commands non-interactively. |
.planning/phases/42-hermes-driver/42-01-PLAN.md:188:| Hermes process stdout → `parse_marker_lines` | Stdout parsing must strictly validate DEVFLOW_RESULT JSON format before advancing commit-gated stages. |
.planning/phases/42-hermes-driver/42-01-PLAN.md:197:| T-42-04 | Repudiation | Conformance suite | low | mitigate | Enroll HermesDriver in conformance suite to verify 7 trait contract checks. |
.planning/phases/42-hermes-driver/42-01-PLAN.md:199:| T-42-06 | Denial of Service | Subagent probe failure | low | mitigate | `hermes_subagent_dispatch_available` fails closed to `false` on any probe failure without halting launch. |
.planning/phases/42-hermes-driver/42-01-PLAN.md:204:1. `cargo test -p devflow-core --lib hermes_driver`
.planning/phases/42-hermes-driver/42-01-PLAN.md:205:2. `cargo test -p devflow-core --lib agent_kind_hermes`
.planning/phases/42-hermes-driver/42-01-PLAN.md:206:3. `cargo test -p devflow-core --lib hermes_conformance_enrollment`
.planning/phases/42-hermes-driver/42-01-PLAN.md:207:4. `cargo test -p devflow --bin devflow doctor_includes_hermes`
.planning/phases/42-hermes-driver/42-01-PLAN.md:208:5. `cargo test -p devflow --test phase7_cli hermes`
.planning/phases/42-hermes-driver/42-01-PLAN.md:212:`AgentKind::Hermes` resolves end-to-end; `HermesDriver` implements the modular driver contract; conformance suite passes with 6 drivers; `devflow doctor` checks Hermes; marker-less, error exit, and hung process regressions pass in `phase7_cli.rs`.
.planning/phases/42-hermes-driver/42-01-PLAN.md:216:Create `.planning/phases/42-hermes-driver/42-01-SUMMARY.md` when done
.planning/phases/42-hermes-driver/42-01-PLAN.md:220:- `HermesDriver` (struct in `crates/devflow-core/src/agents/hermes.rs`)
.planning/phases/42-hermes-driver/42-01-PLAN.md:221:- `AgentKind::Hermes` (enum variant in `crates/devflow-core/src/state.rs`)
.planning/phases/42-hermes-driver/42-01-PLAN.md:222:- `hermes_subagent_dispatch_available` (function in `crates/devflow-core/src/agents/hermes.rs`)
.planning/phases/42-hermes-driver/42-01-PLAN.md:223:- `hermes_conformance_enrollment` (test function in `crates/devflow-core/src/agents/mod.rs`)
.planning/phases/42-hermes-driver/42-01-PLAN.md:224:- `doctor_includes_hermes` (test function in `crates/devflow-cli/src/commands.rs`)
.planning/phases/42-hermes-driver/42-01-PLAN.md:225:- `hermes_marker_less_run_does_not_advance` (test function in `crates/devflow-cli/tests/phase7_cli.rs`)
.planning/phases/42-hermes-driver/42-01-PLAN.md:226:- `hermes_nonzero_exit_does_not_advance` (test function in `crates/devflow-cli/tests/phase7_cli.rs`)
.planning/phases/42-hermes-driver/42-01-PLAN.md:227:- `hermes_hung_process_is_detected_not_left_running` (test function in `crates/devflow-cli/tests/phase7_cli.rs`)
.planning/phases/42-hermes-driver/42-CONTEXT.md:1:# Phase 42: Hermes Driver - Context
.planning/phases/42-hermes-driver/42-CONTEXT.md:9:Phase 42 delivers the modular **Hermes Driver** (`AgentKind::Hermes`), enabling `devflow start --agent hermes` to launch Hermes in oneshot headless mode (`hermes -z "<prompt>" --yolo --accept-hooks`) with honest completion detection and conformance suite enrollment (HRMS-01, HRMS-02, HRMS-03).
.planning/phases/42-hermes-driver/42-CONTEXT.md:18:### Hermes Driver Core & Launch
.planning/phases/42-hermes-driver/42-CONTEXT.md:20:  `HermesDriver::build_command` returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])`.
.planning/phases/42-hermes-driver/42-CONTEXT.md:21:  `HermesDriver::environment` sets `("HERMES_ACCEPT_HOOKS".into(), "1".into())`.
.planning/phases/42-hermes-driver/42-CONTEXT.md:22:  Rationale: `--yolo` bypasses command approvals; `--accept-hooks` prevents interactive TTY prompts on unseen shell hooks configured in `~/.hermes/config.yaml`.
.planning/phases/42-hermes-driver/42-CONTEXT.md:24:  `HermesDriver::render_prompt` delegates to `crate::prompt::render_claude_style(intent)`, providing standard `/gsd-*` commands and the `DEVFLOW_RESULT` completion contract.
.planning/phases/42-hermes-driver/42-CONTEXT.md:26:  `HermesDriver` uses the standard process-exit completion transport: stdout is scanned for `DEVFLOW_RESULT` JSON markers via `parse_marker_lines`. A marker-less run never advances a commit-gated stage (`Plan`, `Code`).
.planning/phases/42-hermes-driver/42-CONTEXT.md:28:  `HermesDriver::capabilities` probes `hermes tools list` for `enabled.*delegation` via a helper `hermes_subagent_dispatch_available()`, setting `subagent_dispatch: true` when enabled and `false` otherwise (mirroring the `pi_subagent_dispatch_available` pattern).
.planning/phases/42-hermes-driver/42-CONTEXT.md:31:- **D-05: Full `AgentKind` registration.** — **Reversibility:** one-way — public enum in `devflow-core`.
.planning/phases/42-hermes-driver/42-CONTEXT.md:32:  Add `AgentKind::Hermes`, wire `FromStr` / `Display`, `driver_for` mapping (`Box::new(HermesDriver)`), and `agent_program` (`"hermes"`).
.planning/phases/42-hermes-driver/42-CONTEXT.md:34:  Add `hermes` to `devflow doctor` checks (`doctor_checks()` in `commands.rs`). Enroll `HermesDriver` in `every_driver_passes_the_conformance_suite` (5 → 6 drivers in `crates/devflow-core/src/agents/mod.rs`).
.planning/phases/42-hermes-driver/42-CONTEXT.md:45:- Exact layout of unit and integration test fixtures for Hermes driver.
.planning/phases/42-hermes-driver/42-CONTEXT.md:46:- Minor doctor version string parsing nuances (`hermes --version` / `hermes -V`).
.planning/phases/42-hermes-driver/42-CONTEXT.md:56:- `.planning/REQUIREMENTS.md` § "Hermes" (HRMS-01..03) & "Antigravity Dogfood + Cadence" (ANTG-04)
.planning/phases/42-hermes-driver/42-CONTEXT.md:57:- `.planning/ROADMAP.md` § "Phase 42: Hermes Driver"
.planning/phases/42-hermes-driver/42-CONTEXT.md:63:- `crates/devflow-cli/src/preflight.rs` — `agent_program` and `unattended_launch_shape_condition` (C2 gate for ANTG-04)
.planning/phases/42-hermes-driver/42-CONTEXT.md:77:- Modular driver pattern (`AgentDriver` impl in `crates/devflow-core/src/agents/hermes.rs`)
.planning/phases/42-hermes-driver/42-CONTEXT.md:82:- `crates/devflow-core/src/state.rs` (`AgentKind::Hermes`)
.planning/phases/42-hermes-driver/42-CONTEXT.md:92:- Ensure `hermes -z "<prompt>" --yolo --accept-hooks` executes cleanly in a headless subshell.
.planning/phases/42-hermes-driver/42-CONTEXT.md:93:- Phase 42 plan structure: 42-01 (Hermes driver implementation & unit/conformance tests) and 42-02 (supervised Antigravity dogfood run & cadence verification).
.planning/phases/42-hermes-driver/42-CONTEXT.md:106:*Phase: 42-Hermes Driver*
.planning/phases/42-hermes-driver/42-VALIDATION.md:3:slug: hermes-driver
.planning/phases/42-hermes-driver/42-VALIDATION.md:24:| **Quick run command** | `cargo test -p devflow-core --lib hermes -- --nocapture && cargo test -p devflow-core --lib hermes_conformance_enrollment -- --nocapture && cargo test -p devflow --bin devflow doctor_includes_hermes -- --nocapture && cargo test -p devflow --test phase7_cli hermes -- --nocapture` |
.planning/phases/42-hermes-driver/42-VALIDATION.md:44:| Hermes subagent delegation probe (D-04) | HRMS-02 | Verify dynamic probe correctly inspects `hermes tools list` | Live: `hermes tools list` contains `enabled delegation`, `HermesDriver.capabilities().subagent_dispatch == true` |
.planning/phases/42-hermes-driver/42-VALIDATION.md:52:| 42-01-01 | 01 | 1 | HRMS-02 | T-42-01/02 | HermesDriver argv (-z, --yolo, --accept-hooks, HERMES_ACCEPT_HOOKS=1) + render_claude_style + subagent capability probe (hermes.rs) | unit | `cargo test -p devflow-core --lib hermes_driver -- --nocapture` | ✅ hermes.rs (new) | ⬜ pending |
.planning/phases/42-hermes-driver/42-VALIDATION.md:53:| 42-01-02 | 01 | 1 | HRMS-01 | T-42-03 | AgentKind variant + FromStr/Display + driver_for dispatch + conformance enrollment 5→6 (state.rs, agents/mod.rs) | unit | `cargo test -p devflow-core --lib agent_kind_hermes -- --nocapture && cargo test -p devflow-core --lib hermes_conformance_enrollment -- --nocapture` | ✅ state.rs / agents/mod.rs | ⬜ pending |
.planning/phases/42-hermes-driver/42-VALIDATION.md:54:| 42-01-03 | 01 | 1 | HRMS-01 | T-42-04 | doctor_checks() seam + hermes entry (commands.rs) | unit | `cargo test -p devflow --bin devflow doctor_includes_hermes -- --nocapture` | ✅ commands.rs | ⬜ pending |
.planning/phases/42-hermes-driver/42-VALIDATION.md:55:| 42-01-04 | 01 | 1 | HRMS-03 | T-42-05/06 | stubbed hermes binary (marker-less, non-zero exit, hung process detection) with MonitorReapGuard (phase7_cli.rs) | integration | `cargo test -p devflow --test phase7_cli hermes -- --nocapture` | ✅ phase7_cli.rs | ⬜ pending |
.planning/phases/42-hermes-driver/42-VALIDATION.md:57:| 42-02-02 | 02 | 2 | ANTG-04 | T-42-08 | Unlock unattended mode for Antigravity in preflight.rs (C2 gate), update tests | unit | `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity_allowed -- --nocapture` | ✅ preflight.rs | ⬜ pending |
.planning/phases/42-hermes-driver/42-VALIDATION.md:63:- [x] `crates/devflow-core/src/agents/hermes.rs` — HermesDriver implementation (argv, prompt rendering, capabilities, environment)
.planning/phases/42-hermes-driver/42-VALIDATION.md:64:- [x] `crates/devflow-core/src/state.rs` — `AgentKind::Hermes` registration
.planning/phases/42-hermes-driver/42-VALIDATION.md:66:- [x] `crates/devflow-cli/src/commands.rs` — `doctor_checks` hermes entry
.planning/phases/42-hermes-driver/42-VALIDATION.md:67:- [x] `crates/devflow-cli/tests/phase7_cli.rs` — hermes stub fixtures and marker-less/exit/hung regression tests
.planning/phases/42-hermes-driver/42-VALIDATION.md:76:| `hermes` live oneshot launch | HRMS-02 | Verify real CLI behavior with `--accept-hooks` | Run `hermes -z "echo test" --yolo --accept-hooks` and confirm execution without interactive prompt |
.planning/phases/42-hermes-driver/42-PATTERNS.md:1:# Phase 42: Hermes Driver - Pattern Map
.planning/phases/42-hermes-driver/42-PATTERNS.md:11:| `crates/devflow-core/src/agents/hermes.rs` | agent driver component | process-exit transport | `crates/devflow-core/src/agents/pi.rs` | exact |
.planning/phases/42-hermes-driver/42-PATTERNS.md:12:| `crates/devflow-core/src/state.rs` (AgentKind variant) | enum variant + Display/FromStr | serialization | `crates/devflow-core/src/state.rs:385-427` | exact |
.planning/phases/42-hermes-driver/42-PATTERNS.md:20:### `crates/devflow-core/src/agents/hermes.rs` (NEW — agent driver, process-exit)
.planning/phases/42-hermes-driver/42-PATTERNS.md:26://! Hermes coding-agent harness adapter (phase 42).
.planning/phases/42-hermes-driver/42-PATTERNS.md:28://! Launches `hermes -z "<prompt>" --yolo --accept-hooks` in headless oneshot mode (D-01).
.planning/phases/42-hermes-driver/42-PATTERNS.md:31://! Probes subagent capabilities via `hermes tools list` looking for enabled delegation (D-04).
.planning/phases/42-hermes-driver/42-PATTERNS.md:40:/// The modular driver for Hermes (Phase 42): oneshot `-z` launch with `--yolo` and
.planning/phases/42-hermes-driver/42-PATTERNS.md:43:pub struct HermesDriver;
.planning/phases/42-hermes-driver/42-PATTERNS.md:45:impl AgentDriver for HermesDriver {
.planning/phases/42-hermes-driver/42-PATTERNS.md:47:        "Hermes"
.planning/phases/42-hermes-driver/42-PATTERNS.md:56:            subagent_dispatch: hermes_subagent_dispatch_available(),
.planning/phases/42-hermes-driver/42-PATTERNS.md:67:            "hermes",
.planning/phases/42-hermes-driver/42-PATTERNS.md:89:fn hermes_subagent_dispatch_available() -> bool {
.planning/phases/42-hermes-driver/42-PATTERNS.md:90:    let Ok(output) = std::process::Command::new("hermes")
.planning/phases/42-hermes-driver/42-PATTERNS.md:99:            .any(|l| l.contains("enabled") && l.contains("delegation"))
.planning/phases/42-hermes-driver/42-PATTERNS.md:105:**Analog:** `AgentKind::Antigravity`
.planning/phases/42-hermes-driver/42-PATTERNS.md:106:- Add `AgentKind::Hermes`
.planning/phases/42-hermes-driver/42-PATTERNS.md:107:- `Display` maps `AgentKind::Hermes` => `"hermes"`
.planning/phases/42-hermes-driver/42-PATTERNS.md:108:- `FromStr` maps `"hermes"` => `Ok(AgentKind::Hermes)`
.planning/phases/42-hermes-driver/42-PATTERNS.md:109:- Error message: `"unsupported agent \`{0}\`; expected claude, codex, opencode, pi, antigravity, or hermes"`
.planning/phases/42-hermes-driver/42-PATTERNS.md:114:- `driver_for(AgentKind::Hermes) => Box::new(HermesDriver)`
.planning/phases/42-hermes-driver/42-PATTERNS.md:116:- Add `hermes_conformance_enrollment` asserting 7 contract checks
.planning/phases/42-hermes-driver/42-PATTERNS.md:123:            "hermes",
.planning/phases/42-hermes-driver/42-PATTERNS.md:124:            "hermes",
.planning/phases/42-hermes-driver/42-PATTERNS.md:126:            "Install Hermes CLI so `hermes` is on PATH",
.planning/phases/42-hermes-driver/42-PATTERNS.md:132:**Analog:** `unattended_launch_shape_condition` (C2 gate)
.planning/phases/42-hermes-driver/42-PATTERNS.md:138:- `hermes_stub(launch: &str)`
.planning/phases/42-hermes-driver/42-PATTERNS.md:139:- `hermes_marker_less_run_does_not_advance()`
.planning/phases/42-hermes-driver/42-PATTERNS.md:140:- `hermes_nonzero_exit_does_not_advance()`
.planning/phases/42-hermes-driver/42-PATTERNS.md:141:- `hermes_hung_process_is_detected_not_left_running()`
.planning/phases/42-hermes-driver/42-RESEARCH.md:1:# Phase 42: Hermes Driver - Research
.planning/phases/42-hermes-driver/42-RESEARCH.md:9:Phase 42 integrates the Hermes Agent (`hermes` v0.20.4) as a modular DevFlow agent driver (`AgentKind::Hermes`), enabling `devflow start --agent hermes` to drive phases in headless oneshot mode (`hermes -z "<prompt>" --yolo --accept-hooks`). The driver renders prompts using `render_claude_style`, sets `HERMES_ACCEPT_HOOKS=1`, detects subagent delegation capability via `hermes tools list`, parses `DEVFLOW_RESULT` from process stdout, and enrolls in the shared conformance suite (HRMS-01, HRMS-02, HRMS-03).
.planning/phases/42-hermes-driver/42-RESEARCH.md:13:**Primary recommendation:** Implement `HermesDriver` in `crates/devflow-core/src/agents/hermes.rs` following the established process-exit modular driver pattern (like `PiDriver`); register `AgentKind::Hermes` across `devflow-core` and `devflow-cli`; enroll `HermesDriver` in the conformance suite (5 → 6 drivers); add `hermes` to `devflow doctor`; test stubbed failure modes in `phase7_cli.rs`; and execute the implementation via supervised Antigravity dogfooding to satisfy ANTG-04.
.planning/phases/42-hermes-driver/42-RESEARCH.md:20:  `HermesDriver::build_command` returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])`.
.planning/phases/42-hermes-driver/42-RESEARCH.md:21:  `HermesDriver::environment` sets `("HERMES_ACCEPT_HOOKS".into(), "1".into())`.
.planning/phases/42-hermes-driver/42-RESEARCH.md:22:  `--yolo` bypasses command approvals; `--accept-hooks` prevents interactive TTY prompts on unseen shell hooks configured in `~/.hermes/config.yaml`. [VERIFIED: CONTEXT.md:19-22]
.planning/phases/42-hermes-driver/42-RESEARCH.md:25:  `HermesDriver::render_prompt` delegates to `crate::prompt::render_claude_style(intent)`, providing standard `/gsd-*` commands and the `DEVFLOW_RESULT` completion contract. [VERIFIED: CONTEXT.md:23-24]
.planning/phases/42-hermes-driver/42-RESEARCH.md:28:  `HermesDriver` uses the standard process-exit completion transport: stdout is scanned for `DEVFLOW_RESULT` JSON markers via `parse_marker_lines`. A marker-less run never advances a commit-gated stage (`Plan`, `Code`). [VERIFIED: CONTEXT.md:25-27]
.planning/phases/42-hermes-driver/42-RESEARCH.md:31:  `HermesDriver::capabilities` probes `hermes tools list` for `enabled.*delegation` via a helper `hermes_subagent_dispatch_available()`, setting `subagent_dispatch: true` when enabled and `false` otherwise (mirroring the `pi_subagent_dispatch_available` pattern). [VERIFIED: CONTEXT.md:28-29]
.planning/phases/42-hermes-driver/42-RESEARCH.md:33:- **D-05: Full `AgentKind` registration.**
.planning/phases/42-hermes-driver/42-RESEARCH.md:34:  Add `AgentKind::Hermes`, wire `FromStr` / `Display`, `driver_for` mapping (`Box::new(HermesDriver)`), and `agent_program` (`"hermes"`). [VERIFIED: CONTEXT.md:31-32]
.planning/phases/42-hermes-driver/42-RESEARCH.md:37:  Add `hermes` to `devflow doctor` checks (`doctor_checks()` in `commands.rs`). Enroll `HermesDriver` in `every_driver_passes_the_conformance_suite` (5 → 6 drivers in `crates/devflow-core/src/agents/mod.rs`), verified by `hermes_conformance_enrollment`. [VERIFIED: CONTEXT.md:33-34]
.planning/phases/42-hermes-driver/42-RESEARCH.md:48:- Exact layout of unit and integration test fixtures for Hermes driver.
.planning/phases/42-hermes-driver/42-RESEARCH.md:49:- Minor doctor version string parsing nuances (`hermes --version` / `hermes -V`).
.planning/phases/42-hermes-driver/42-RESEARCH.md:58:| HRMS-01 | Operator can select `--agent hermes` — full `AgentKind` registration | Add `AgentKind::Hermes`, wire `FromStr`/`Display`, `driver_for`, `agent_program` |
.planning/phases/42-hermes-driver/42-RESEARCH.md:59:| HRMS-02 | Hermes driver launches headless (`hermes -z "<prompt>" --yolo --accept-hooks`) and passes conformance suite | D-01 argv contract, D-02 `render_claude_style`, D-06 conformance enrollment 5→6 |
.planning/phases/42-hermes-driver/42-RESEARCH.md:60:| HRMS-03 | Hermes completion is honest (process-exit + `DEVFLOW_RESULT` prompt contract); marker-less run never advances | D-03 process-exit parsing via `parse_marker_lines` + `phase7_cli.rs` stubbed-PATH regression tests |
.planning/phases/42-hermes-driver/42-RESEARCH.md:72:| Prompt rendering | Driver (`HermesDriver`) | Stage intent → prompt text | `render_claude_style` produces slash-command prompts with DEVFLOW_RESULT instructions |
.planning/phases/42-hermes-driver/42-RESEARCH.md:73:| Capability discovery | Driver (`capabilities()`) | CLI probe (`hermes tools list`) | Dynamic detection checks if delegation toolset is enabled in Hermes |
.planning/phases/42-hermes-driver/42-RESEARCH.md:74:| Conformance validation | Shared trait methods (`test_contract`) | Driver unit tests | Every driver must pass 7 contract checks (name, prompts for all stages, program name) |
.planning/phases/42-hermes-driver/42-RESEARCH.md:83:| Hermes CLI | `/home/denniyahh/.local/bin/hermes` / `hermes` (v0.20.4) | Headless agent execution | Operator's installed Hermes Agent |
.planning/phases/42-hermes-driver/42-RESEARCH.md:91:| `AgentDriver` trait | `crates/devflow-core/src/agents/mod.rs` | Modular driver interface | Implemented by `HermesDriver` |
.planning/phases/42-hermes-driver/42-RESEARCH.md:92:| `AgentKind` enum | `crates/devflow-core/src/state.rs` | Agent enumeration | Extended with `AgentKind::Hermes` |
.planning/phases/42-hermes-driver/42-RESEARCH.md:94:| `doctor_checks` | `crates/devflow-cli/src/commands.rs` | Environment diagnostics | Added `hermes` presence probe |
.planning/phases/42-hermes-driver/42-RESEARCH.md:95:| `unattended_launch_shape_condition` | `crates/devflow-cli/src/preflight.rs` | Auto-mode preflight gate | Updated when ANTG-04 dogfooding requirement is satisfied |
.planning/phases/42-hermes-driver/42-RESEARCH.md:99:This phase installs **no new packages**. All Hermes integrations and Antigravity dogfooding validations are process/argv-level and documentation/test fixtures.
.planning/phases/42-hermes-driver/42-RESEARCH.md:103:### Modular Driver Pattern (`HermesDriver`)
.planning/phases/42-hermes-driver/42-RESEARCH.md:104:`HermesDriver` implements `AgentDriver` in `crates/devflow-core/src/agents/hermes.rs`:
.planning/phases/42-hermes-driver/42-RESEARCH.md:105:- `name()`: returns `"Hermes"`
.planning/phases/42-hermes-driver/42-RESEARCH.md:107:- `build_command(&self, phase, prompt, extra_roots)`: returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])`
.planning/phases/42-hermes-driver/42-RESEARCH.md:109:- `capabilities(&self)`: returns `DriverCapabilities { subagent_dispatch: hermes_subagent_dispatch_available() }`
.planning/phases/42-hermes-driver/42-RESEARCH.md:113:`hermes_subagent_dispatch_available()` executes `hermes tools list` and checks if output contains `enabled` and `delegation`. Fails closed (`false`) on non-zero exit or error.
.planning/phases/42-hermes-driver/42-RESEARCH.md:116:`crates/devflow-core/src/agents/mod.rs` updates `every_driver_passes_the_conformance_suite` to include `Box::new(HermesDriver)` in its 6-element array, and adds `hermes_conformance_enrollment` asserting 7 contract checks.
.planning/phases/42-hermes-driver/42-RESEARCH.md:119:Once Phase 42 executes successfully under supervised Antigravity (`devflow start --agent antigravity --phase 42 --mode supervise`), Antigravity dogfooding is complete. `preflight.rs`'s `unattended_launch_shape_condition` can be updated to include Antigravity alongside Claude as dogfooded stream agents.
.planning/phases/42-hermes-driver/42-RESEARCH.md:127:cargo test -p devflow-core --lib hermes -- --nocapture && cargo test -p devflow-core --lib hermes_conformance_enrollment -- --nocapture && cargo test -p devflow --bin devflow doctor_includes_hermes -- --nocapture && cargo test -p devflow --test phase7_cli hermes -- --nocapture
.planning/phases/42-hermes-driver/42-RESEARCH.md:136:1. A marker-less `hermes` run must fail closed at commit-gated stages (`Plan`, `Code`).
.planning/phases/42-hermes-driver/42-RESEARCH.md:137:2. A non-zero exit code from `hermes` must gate the stage.
.planning/phases/42-hermes-driver/42-RESEARCH.md:138:3. A hung `hermes` process must be detected and killed, not orphaned.
.planning/phases/42-hermes-driver/42-RESEARCH.md:139:4. An unparseable `hermes tools list` output must fail closed (`subagent_dispatch: false`).
.planning/phases/42-hermes-driver/42-UAT.md:3:phase: 42-hermes-driver
.planning/phases/42-hermes-driver/42-UAT.md:14:  All Hermes driver unit tests, conformance checks, doctor checks, transport
.planning/phases/42-hermes-driver/42-UAT.md:20:### 1. HermesDriver modular implementation (HRMS-01, D-01, D-02, D-04)
.planning/phases/42-hermes-driver/42-UAT.md:22:evidence: `cargo test -p devflow-core --lib hermes` → 14 passed (command shape, environment `HERMES_ACCEPT_HOOKS=1`, prompt rendering, and delegation probing).
.planning/phases/42-hermes-driver/42-UAT.md:24:### 2. AgentKind registration & 6-driver conformance enrollment (HRMS-01, D-05, D-06)
.planning/phases/42-hermes-driver/42-UAT.md:26:evidence: `cargo test -p devflow-core --lib agent_kind_hermes` (5 passed), `hermes_conformance_enrollment` (1 passed, all 6 drivers pass 7 contract checks).
.planning/phases/42-hermes-driver/42-UAT.md:30:evidence: `cargo test -p devflow --bin devflow doctor_includes_hermes` → 1 passed.
.planning/phases/42-hermes-driver/42-UAT.md:32:### 4. Transport integration regressions with MonitorReapGuard (HRMS-03, D-03)
.planning/phases/42-hermes-driver/42-UAT.md:34:evidence: `cargo test -p devflow --test phase7_cli hermes` → 3 passed (`hermes_marker_less_run_does_not_advance`, `hermes_nonzero_exit_does_not_advance`, `hermes_hung_process_is_detected_not_left_running`).
.planning/phases/42-hermes-driver/42-UAT.md:42:evidence: `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity` → 1 passed.
.planning/phases/42-hermes-driver/review_codex.md:13:You are an adversarial CODE reviewer. Attack the implementation of Phase 42 (Hermes Agent Driver & Antigravity Unattended Mode Graduation) in this repository. Do not summarize or praise the code. Your job is to find real, concrete defects with the exact file and line/quoted text for each finding.
.planning/phases/42-hermes-driver/review_codex.md:16:- crates/devflow-core/src/agents/hermes.rs
.planning/phases/42-hermes-driver/review_codex.md:22:- .planning/phases/42-hermes-driver/42-01-PLAN.md
.planning/phases/42-hermes-driver/review_codex.md:23:- .planning/phases/42-hermes-driver/42-02-PLAN.md
.planning/phases/42-hermes-driver/review_codex.md:24:- .planning/phases/42-hermes-driver/42-VERIFICATION.md
.planning/phases/42-hermes-driver/review_codex.md:25:- .planning/phases/42-hermes-driver/42-UAT.md
.planning/phases/42-hermes-driver/review_codex.md:28:- Goal: Implement modular HermesDriver for Hermes Agent CLI (`hermes -z "<prompt>" --yolo --accept-hooks`), with `HERMES_ACCEPT_HOOKS=1`, dynamic `hermes tools list` delegation probe, presence-only health probe, registration of AgentKind::Hermes, 6-driver conformance suite enrollment, devflow doctor presence probe, transport integration tests with MonitorReapGuard, and unlocking unattended auto mode for Antigravity in preflight.rs (C2 gate) following supervised dogfooding.
.planning/phases/42-hermes-driver/review_codex.md:31:1. Subprocess & argument construction bugs: wrong flags for hermes spawn (`-z`, `--yolo`, `--accept-hooks`), prompt parameter carrier, quote escaping, missing `--` separator if needed, environment inheritance.
.planning/phases/42-hermes-driver/review_codex.md:33:3. Preflight & condition state correctness: does `unattended_launch_shape_condition` correctly permit Antigravity while continuing to refuse unsupported/non-stream agents and legacy opt-outs?
.planning/phases/42-hermes-driver/review_codex.md:34:4. Dynamic capabilities detection: parsing `hermes tools list` output for delegation toolset, case sensitivity, failure modes if CLI is absent/errors.
.planning/phases/42-hermes-driver/review_codex.md:35:5. Error handling, exit codes, signal killing, and monitor liveness: unhandled error envelopes, hung process cleanup, MonitorReapGuard leakage.
.planning/phases/42-hermes-driver/review_codex.md:36:6. test_contract() and conformance suite assertions: are assertions meaningful with negative controls?
.planning/phases/42-hermes-driver/review_codex.md:37:7. Concurrency / state machine transitions: state serialization/deserialization, gate resolution, stop_until behavior.
.planning/phases/42-hermes-driver/review_codex.md:117:   - Approval requires explicit evidence from both independent lanes; missing or failed delegation is a blocking unavailable-review state, not an approval fallback
.planning/phases/42-hermes-driver/review_codex.md:313:/bin/bash -lc 'git diff --stat && nl -ba crates/devflow-core/src/agents/hermes.rs && nl -ba crates/devflow-core/src/agents/mod.rs && nl -ba crates/devflow-core/src/state.rs' in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
.planning/phases/42-hermes-driver/review_codex.md:315:     1	//! Hermes coding-agent adapter (Phase 42).
.planning/phases/42-hermes-driver/review_codex.md:317:     3	//! Launches `hermes -z "<prompt>" --yolo --accept-hooks` in headless-safe oneshot mode.
.planning/phases/42-hermes-driver/review_codex.md:322:     8	//! Subagent dispatch capability is dynamically probed via `hermes tools list` checking for
.planning/phases/42-hermes-driver/review_codex.md:323:     9	//! the enabled `delegation` toolset.
.planning/phases/42-hermes-driver/review_codex.md:329:    15	/// The modular driver for Hermes (Phase 42): headless `-z` oneshot launch,
.planning/phases/42-hermes-driver/review_codex.md:331:    17	/// and dynamic delegation subagent dispatch probing.
.planning/phases/42-hermes-driver/review_codex.md:332:    18	pub struct HermesDriver;
.planning/phases/42-hermes-driver/review_codex.md:334:    20	impl AgentDriver for HermesDriver {
.planning/phases/42-hermes-driver/review_codex.md:336:    22	        "Hermes"
.planning/phases/42-hermes-driver/review_codex.md:345:    31	            subagent_dispatch: hermes_subagent_dispatch_available(),
.planning/phases/42-hermes-driver/review_codex.md:356:    42	            "hermes",
.planning/phases/42-hermes-driver/review_codex.md:371:    57	        // Presence-only probe of the `hermes` binary.
.planning/phases/42-hermes-driver/review_codex.md:372:    58	        let output = std::process::Command::new("hermes")
.planning/phases/42-hermes-driver/review_codex.md:375:    61	            .map_err(|e| format!("could not run `hermes --version`: {e}"))?;
.planning/phases/42-hermes-driver/review_codex.md:380:    66	            Err(format!("`hermes --version` failed: {}", detail.trim()))
.planning/phases/42-hermes-driver/review_codex.md:385:    71	/// Dynamically probe whether Hermes has the `delegation` toolset enabled.
.planning/phases/42-hermes-driver/review_codex.md:387:    73	/// Runs `hermes tools list` and checks for both `enabled` and `delegation` in the output.
.planning/phases/42-hermes-driver/review_codex.md:388:    74	pub fn hermes_subagent_dispatch_available() -> bool {
.planning/phases/42-hermes-driver/review_codex.md:389:    75	    hermes_subagent_dispatch_available_with(|| {
.planning/phases/42-hermes-driver/review_codex.md:390:    76	        std::process::Command::new("hermes")
.planning/phases/42-hermes-driver/review_codex.md:397:    83	pub fn hermes_subagent_dispatch_available_with(
.planning/phases/42-hermes-driver/review_codex.md:407:    93	    parse_hermes_tools_list_for_delegation(&stdout)
.planning/phases/42-hermes-driver/review_codex.md:410:    96	/// Parse `hermes tools list` stdout to check if delegation toolset is enabled.
.planning/phases/42-hermes-driver/review_codex.md:411:    97	pub fn parse_hermes_tools_list_for_delegation(stdout: &str) -> bool {
.planning/phases/42-hermes-driver/review_codex.md:414:   100	        if lower.contains("delegation") && lower.contains("enabled") {
.planning/phases/42-hermes-driver/review_codex.md:429:   115	    fn hermes_driver_name() {
.planning/phases/42-hermes-driver/review_codex.md:430:   116	        let driver = HermesDriver;
.planning/phases/42-hermes-driver/review_codex.md:431:   117	        assert_eq!(driver.name(), "Hermes");
.planning/phases/42-hermes-driver/review_codex.md:435:   121	    fn hermes_driver_build_command() {
.planning/phases/42-hermes-driver/review_codex.md:436:   122	        let driver = HermesDriver;
.planning/phases/42-hermes-driver/review_codex.md:438:   124	        assert_eq!(prog, "hermes");
.planning/phases/42-hermes-driver/review_codex.md:451:   137	    fn hermes_driver_environment() {
.planning/phases/42-hermes-driver/review_codex.md:452:   138	        let driver = HermesDriver;
.planning/phases/42-hermes-driver/review_codex.md:458:   144	    fn hermes_driver_render_prompt() {
.planning/phases/42-hermes-driver/review_codex.md:459:   145	        let driver = HermesDriver;
.planning/phases/42-hermes-driver/review_codex.md:467:   153	    fn parse_hermes_tools_list_delegation_enabled() {
.planning/phases/42-hermes-driver/review_codex.md:470:   156	  ✓ enabled delegation 👥 Task Delegation
.planning/phases/42-hermes-driver/review_codex.md:474:   160	        assert!(parse_hermes_tools_list_for_delegation(sample));
.planning/phases/42-hermes-driver/review_codex.md:478:   164	    fn parse_hermes_tools_list_delegation_disabled() {
.planning/phases/42-hermes-driver/review_codex.md:481:   167	  ✗ disabled delegation 👥 Task Delegation
.planning/phases/42-hermes-driver/review_codex.md:484:   170	        assert!(!parse_hermes_tools_list_for_delegation(sample));
.planning/phases/42-hermes-driver/review_codex.md:488:   174	    fn parse_hermes_tools_list_missing_delegation() {
.planning/phases/42-hermes-driver/review_codex.md:493:   179	        assert!(!parse_hermes_tools_list_for_delegation(sample));
.planning/phases/42-hermes-driver/review_codex.md:497:   183	    fn hermes_subagent_dispatch_with_mock() {
.planning/phases/42-hermes-driver/review_codex.md:501:   187	                stdout: b"  \xe2\x9c\x93 enabled delegation \xf0\x9f\x91\xa5 Task Delegation\n".to_vec(),
.planning/phases/42-hermes-driver/review_codex.md:505:   191	        assert!(hermes_subagent_dispatch_available_with(success_output));
.planning/phases/42-hermes-driver/review_codex.md:514:   200	        assert!(!hermes_subagent_dispatch_available_with(failure_output));
.planning/phases/42-hermes-driver/review_codex.md:517:   203	        assert!(!hermes_subagent_dispatch_available_with(io_error));
.planning/phases/42-hermes-driver/review_codex.md:528:     9	use crate::state::AgentKind;
.planning/phases/42-hermes-driver/review_codex.md:636:   117	    fn test_contract(&self) -> Vec<ContractResult> {
.planning/phases/42-hermes-driver/review_codex.md:662:   143	/// Shared conformance checks every driver's `test_contract` runs (37-04).
.planning/phases/42-hermes-driver/review_codex.md:663:   144	/// A future driver (Antigravity, Hermes) plugs in by passing these — the
.planning/phases/42-hermes-driver/review_codex.md:693:   174	pub fn driver_for(kind: AgentKind) -> Box<dyn AgentDriver> {
.planning/phases/42-hermes-driver/review_codex.md:695:   176	        AgentKind::Claude => Box::new(ClaudeDriver),
.planning/phases/42-hermes-driver/review_codex.md:696:   177	        AgentKind::Codex => Box::new(CodexDriver),
.planning/phases/42-hermes-driver/review_codex.md:697:   178	        AgentKind::OpenCode => Box::new(OpenCodeDriver),
.planning/phases/42-hermes-driver/review_codex.md:698:   179	        AgentKind::Pi => Box::new(PiDriver),
.planning/phases/42-hermes-driver/review_codex.md:699:   180	        AgentKind::Antigravity => Box::new(AntigravityDriver),
.planning/phases/42-hermes-driver/review_codex.md:700:   181	        AgentKind::Hermes => Box::new(HermesDriver),
.planning/phases/42-hermes-driver/review_codex.md:707:   188	pub mod hermes;
.planning/phases/42-hermes-driver/review_codex.md:714:   195	pub use hermes::HermesDriver;
.planning/phases/42-hermes-driver/review_codex.md:726:   207	        assert_eq!(driver_for(AgentKind::Claude).name(), "Claude Code");
.planning/phases/42-hermes-driver/review_codex.md:727:   208	        assert_eq!(driver_for(AgentKind::Codex).name(), "OpenAI Codex");
.planning/phases/42-hermes-driver/review_codex.md:728:   209	        assert_eq!(driver_for(AgentKind::OpenCode).name(), "OpenCode");
.planning/phases/42-hermes-driver/review_codex.md:729:   210	        assert_eq!(driver_for(AgentKind::Pi).name(), "Pi");
.planning/phases/42-hermes-driver/review_codex.md:730:   211	        assert_eq!(driver_for(AgentKind::Antigravity).name(), "Antigravity");
.planning/phases/42-hermes-driver/review_codex.md:731:   212	        assert_eq!(driver_for(AgentKind::Hermes).name(), "Hermes");
.planning/phases/42-hermes-driver/review_codex.md:811:   292	            Box::new(HermesDriver),
.planning/phases/42-hermes-driver/review_codex.md:814:   295	            let results = driver.test_contract();
.planning/phases/42-hermes-driver/review_codex.md:849:   330	        let results = antigravity.test_contract();
.planning/phases/42-hermes-driver/review_codex.md:865:   346	                driver.test_contract().iter().all(|r| r.passed),
.planning/phases/42-hermes-driver/review_codex.md:872:   353	    /// Phase 42 Task 2 (D-06): the Hermes enrollment is PROVEN by a uniquely-named test.
.planning/phases/42-hermes-driver/review_codex.md:873:   354	    /// Asserts the hardcoded array is now 6 drivers AND that the Hermes driver
.planning/phases/42-hermes-driver/review_codex.md:876:   357	    fn hermes_conformance_enrollment() {
.planning/phases/42-hermes-driver/review_codex.md:883:   364	            Box::new(HermesDriver),
.planning/phases/42-hermes-driver/review_codex.md:885:   366	        let hermes = drivers
.planning/phases/42-hermes-driver/review_codex.md:887:   368	            .find(|d| d.name() == "Hermes")
.planning/phases/42-hermes-driver/review_codex.md:888:   369	            .expect("the Hermes driver must be enrolled in the shared suite");
.planning/phases/42-hermes-driver/review_codex.md:889:   370	        let results = hermes.test_contract();
.planning/phases/42-hermes-driver/review_codex.md:898:   379	                "Hermes failed conformance case {:?}",
.planning/phases/42-hermes-driver/review_codex.md:905:   386	                driver.test_contract().iter().all(|r| r.passed),
.planning/phases/42-hermes-driver/review_codex.md:913:   394	    /// must FAIL it — the negative control proving `test_contract` isn't
.planning/phases/42-hermes-driver/review_codex.md:936:   417	        let results = BrokenDriver.test_contract();
.planning/phases/42-hermes-driver/review_codex.md:1005:   486	        let claude = driver_for(AgentKind::Claude).render_prompt(&intent);
.planning/phases/42-hermes-driver/review_codex.md:1006:   487	        let opencode = driver_for(AgentKind::OpenCode).render_prompt(&intent);
.planning/phases/42-hermes-driver/review_codex.md:1007:   488	        let codex = driver_for(AgentKind::Codex).render_prompt(&intent);
.planning/phases/42-hermes-driver/review_codex.md:1058:   539	            driver_for(AgentKind::Claude).build_command(PhaseId::new(3), &prompt, &[]);
.planning/phases/42-hermes-driver/review_codex.md:1086:   567	            driver_for(AgentKind::Codex).build_command(PhaseId::new(7), &prompt, &[]);
.planning/phases/42-hermes-driver/review_codex.md:1098:   579	            driver_for(AgentKind::OpenCode).build_command(PhaseId::new(7), &prompt, &[]);
.planning/phases/42-hermes-driver/review_codex.md:1116:   597	            driver_for(AgentKind::Codex).build_command(PhaseId::new(7), &prompt, &roots);
.planning/phases/42-hermes-driver/review_codex.md:1125:   606	        let (_, args) = driver_for(AgentKind::Codex).build_command(PhaseId::new(7), &prompt, &[]);
.planning/phases/42-hermes-driver/review_codex.md:1138:   619	        let env = driver_for(AgentKind::Codex).environment();
.planning/phases/42-hermes-driver/review_codex.md:1141:   622	        assert!(driver_for(AgentKind::Claude).environment().is_empty());
.planning/phases/42-hermes-driver/review_codex.md:1142:   623	        assert!(driver_for(AgentKind::OpenCode).environment().is_empty());
.planning/phases/42-hermes-driver/review_codex.md:1153:   634	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1157:   638	        assert!(driver_for(AgentKind::Claude).health(&state).is_ok());
.planning/phases/42-hermes-driver/review_codex.md:1158:   639	        assert!(driver_for(AgentKind::Codex).health(&state).is_ok());
.planning/phases/42-hermes-driver/review_codex.md:1159:   640	        assert!(driver_for(AgentKind::OpenCode).health(&state).is_ok());
.planning/phases/42-hermes-driver/review_codex.md:1188:    27	/// adds a run-scoped concept (`worktree_path`, `monitor_pid`, `stop_until`,
.planning/phases/42-hermes-driver/review_codex.md:1202:    41	    pub agent: AgentKind,
.planning/phases/42-hermes-driver/review_codex.md:1480:   319	    pub stop_until: Option<Stage>,
.planning/phases/42-hermes-driver/review_codex.md:1481:   320	    /// Set by `transition()` when `stop_until` names the stage just
.planning/phases/42-hermes-driver/review_codex.md:1538:   377	    /// `devflow resume` would be the same silent-drop class as `stop_until`'s
.planning/phases/42-hermes-driver/review_codex.md:1548:   387	pub enum AgentKind {
.planning/phases/42-hermes-driver/review_codex.md:1559:   398	    /// Hermes Agent CLI (`hermes`), headless oneshot (phase 42).
.planning/phases/42-hermes-driver/review_codex.md:1560:   399	    Hermes,
.planning/phases/42-hermes-driver/review_codex.md:1563:   402	impl fmt::Display for AgentKind {
.planning/phases/42-hermes-driver/review_codex.md:1566:   405	            AgentKind::Claude => "claude",
.planning/phases/42-hermes-driver/review_codex.md:1567:   406	            AgentKind::Codex => "codex",
.planning/phases/42-hermes-driver/review_codex.md:1568:   407	            AgentKind::OpenCode => "opencode",
.planning/phases/42-hermes-driver/review_codex.md:1569:   408	            AgentKind::Pi => "pi",
.planning/phases/42-hermes-driver/review_codex.md:1570:   409	            AgentKind::Antigravity => "antigravity",
.planning/phases/42-hermes-driver/review_codex.md:1571:   410	            AgentKind::Hermes => "hermes",
.planning/phases/42-hermes-driver/review_codex.md:1577:   416	impl FromStr for AgentKind {
.planning/phases/42-hermes-driver/review_codex.md:1582:   421	            "claude" => Ok(AgentKind::Claude),
.planning/phases/42-hermes-driver/review_codex.md:1583:   422	            "codex" => Ok(AgentKind::Codex),
.planning/phases/42-hermes-driver/review_codex.md:1584:   423	            "opencode" | "open-code" => Ok(AgentKind::OpenCode),
.planning/phases/42-hermes-driver/review_codex.md:1585:   424	            "pi" => Ok(AgentKind::Pi),
.planning/phases/42-hermes-driver/review_codex.md:1586:   425	            "antigravity" => Ok(AgentKind::Antigravity),
.planning/phases/42-hermes-driver/review_codex.md:1587:   426	            "hermes" => Ok(AgentKind::Hermes),
.planning/phases/42-hermes-driver/review_codex.md:1595:   434	#[error("unsupported agent `{0}`; expected claude, codex, opencode, pi, antigravity, or hermes")]
.planning/phases/42-hermes-driver/review_codex.md:1600:   439	    pub fn new(phase: PhaseId, agent: AgentKind, mode: Mode, project_root: PathBuf) -> Self {
.planning/phases/42-hermes-driver/review_codex.md:1622:   461	            stop_until: None,
.planning/phases/42-hermes-driver/review_codex.md:1647:   486	        assert_eq!(driver_for(AgentKind::Claude).name(), "Claude Code");
.planning/phases/42-hermes-driver/review_codex.md:1648:   487	        assert_eq!(driver_for(AgentKind::Codex).name(), "OpenAI Codex");
.planning/phases/42-hermes-driver/review_codex.md:1649:   488	        assert_eq!(driver_for(AgentKind::OpenCode).name(), "OpenCode");
.planning/phases/42-hermes-driver/review_codex.md:1651:   490	        assert_eq!(AgentKind::Claude.to_string(), "claude");
.planning/phases/42-hermes-driver/review_codex.md:1652:   491	        assert_eq!(AgentKind::Codex.to_string(), "codex");
.planning/phases/42-hermes-driver/review_codex.md:1653:   492	        assert_eq!(AgentKind::OpenCode.to_string(), "opencode");
.planning/phases/42-hermes-driver/review_codex.md:1658:   497	        assert_eq!("claude".parse::<AgentKind>().unwrap(), AgentKind::Claude);
.planning/phases/42-hermes-driver/review_codex.md:1659:   498	        assert_eq!("CLAUDE".parse::<AgentKind>().unwrap(), AgentKind::Claude);
.planning/phases/42-hermes-driver/review_codex.md:1660:   499	        assert_eq!("codex".parse::<AgentKind>().unwrap(), AgentKind::Codex);
.planning/phases/42-hermes-driver/review_codex.md:1662:   501	            "opencode".parse::<AgentKind>().unwrap(),
.planning/phases/42-hermes-driver/review_codex.md:1663:   502	            AgentKind::OpenCode
.planning/phases/42-hermes-driver/review_codex.md:1666:   505	            "open-code".parse::<AgentKind>().unwrap(),
.planning/phases/42-hermes-driver/review_codex.md:1667:   506	            AgentKind::OpenCode
.planning/phases/42-hermes-driver/review_codex.md:1673:   512	        let err = "aider".parse::<AgentKind>().unwrap_err();
.planning/phases/42-hermes-driver/review_codex.md:1681:   520	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1687:   526	        assert_eq!(state.agent, AgentKind::Claude);
.planning/phases/42-hermes-driver/review_codex.md:1696:   535	        assert_eq!(state.stop_until, None);
.planning/phases/42-hermes-driver/review_codex.md:1706:   545	            AgentKind::Codex,
.planning/phases/42-hermes-driver/review_codex.md:1713:   552	        assert_eq!(back.agent, AgentKind::Codex);
.planning/phases/42-hermes-driver/review_codex.md:1722:   561	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1745:   584	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1788:   627	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1833:   672	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1878:   717	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1930:   769	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1953:   792	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:1999:   838	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:2032:   871	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:2073:   912	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:2113:   952	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:2153:   992	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:2185:  1024	    /// 20c: `stop_until`/`stopped`/`stop_reason` all round-trip through
.planning/phases/42-hermes-driver/review_codex.md:2192:  1031	            AgentKind::Claude,
.planning/phases/42-hermes-driver/review_codex.md:2196:  1035	        state.stop_until = Some(Stage::Plan);
.planning/phases/42-hermes-driver/review_codex.md:2201:  1040	            json.contains("stop_until") && json.contains("stopped") && json.contains("stop_reason"),
.planning/phases/42-hermes-driver/review_codex.md:2206:  1045	            loaded.stop_until,
.planning/phases/42-hermes-driver/review_codex.md:2208:  1047	            "stop_until must round-trip through serde"
.planning/phases/42-hermes-driver/review_codex.md:2218:  1057	    /// A serde-absent `stop_until`/`stopped`/`stop_reason` (state written by
.planning/phases/42-hermes-driver/review_codex.md:2233:  1072	        assert_eq!(loaded.stop_until, None);
.planning/phases/42-hermes-driver/review_codex.md:2239:  1078	    // Phase 41 Task 6: AgentKind::Antigravity (unique test prefix so the
.planning/phases/42-hermes-driver/review_codex.md:2246:  1085	            "antigravity".parse::<AgentKind>().unwrap(),
.planning/phases/42-hermes-driver/review_codex.md:2247:  1086	            AgentKind::Antigravity
.planning/phases/42-hermes-driver/review_codex.md:2250:  1089	            "ANTIGRAVITY".parse::<AgentKind>().unwrap(),
.planning/phases/42-hermes-driver/review_codex.md:2251:  1090	            AgentKind::Antigravity
.planning/phases/42-hermes-driver/review_codex.md:2254:  1093	            "Antigravity".parse::<AgentKind>().unwrap(),
.planning/phases/42-hermes-driver/review_codex.md:2255:  1094	            AgentKind::Antigravity
.planning/phases/42-hermes-driver/review_codex.md:2261:  1100	        assert_eq!(AgentKind::Antigravity.to_string(), "antigravity");
.planning/phases/42-hermes-driver/review_codex.md:2266:  1105	        let wire = serde_json::to_string(&AgentKind::Antigravity).unwrap();
.planning/phases/42-hermes-driver/review_codex.md:2268:  1107	        let back: AgentKind = serde_json::from_str(&wire).unwrap();
.planning/phases/42-hermes-driver/review_codex.md:2269:  1108	        assert_eq!(back, AgentKind::Antigravity);
.planning/phases/42-hermes-driver/review_codex.md:2274:  1113	        let err = "aider".parse::<AgentKind>().unwrap_err();
.planning/phases/42-hermes-driver/review_codex.md:2284:  1123	            crate::agents::driver_for(AgentKind::Antigravity).name(),
.planning/phases/42-hermes-driver/review_codex.md:2290:  1129	    // Phase 42 Task 2: AgentKind::Hermes (HRMS-01, D-05, D-06).
.planning/phases/42-hermes-driver/review_codex.md:2294:  1133	    fn agent_kind_hermes_from_str_is_case_insensitive() {
.planning/phases/42-hermes-driver/review_codex.md:2295:  1134	        assert_eq!("hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
.planning/phases/42-hermes-driver/review_codex.md:2296:  1135	        assert_eq!("HERMES".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
.planning/phases/42-hermes-driver/review_codex.md:2297:  1136	        assert_eq!("Hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
.planning/phases/42-hermes-driver/review_codex.md:2301:  1140	    fn agent_kind_hermes_display_is_lowercase() {
.planning/phases/42-hermes-driver/review_codex.md:2302:  1141	        assert_eq!(AgentKind::Hermes.to_string(), "hermes");
.planning/phases/42-hermes-driver/review_codex.md:2306:  1145	    fn agent_kind_hermes_serde_round_trips_lowercase() {
.planning/phases/42-hermes-driver/review_codex.md:2307:  1146	        let wire = serde_json::to_string(&AgentKind::Hermes).unwrap();
.planning/phases/42-hermes-driver/review_codex.md:2308:  1147	        assert_eq!(wire, "\"hermes\"");
.planning/phases/42-hermes-driver/review_codex.md:2309:  1148	        let back: AgentKind = serde_json::from_str(&wire).unwrap();
.planning/phases/42-hermes-driver/review_codex.md:2310:  1149	        assert_eq!(back, AgentKind::Hermes);
.planning/phases/42-hermes-driver/review_codex.md:2314:  1153	    fn agent_kind_hermes_error_message_lists_it() {
.planning/phases/42-hermes-driver/review_codex.md:2315:  1154	        let err = "aider".parse::<AgentKind>().unwrap_err();
.planning/phases/42-hermes-driver/review_codex.md:2317:  1156	            err.to_string().contains("hermes"),
.planning/phases/42-hermes-driver/review_codex.md:2323:  1162	    fn agent_kind_hermes_driver_for_resolves() {
.planning/phases/42-hermes-driver/review_codex.md:2325:  1164	            crate::agents::driver_for(AgentKind::Hermes).name(),
.planning/phases/42-hermes-driver/review_codex.md:2326:  1165	            "Hermes"
.planning/phases/42-hermes-driver/42-VERIFICATION.md:1:# Phase 42: Hermes Driver & Antigravity Dogfood Verification
.planning/phases/42-hermes-driver/42-VERIFICATION.md:5:- **Phase**: 42 (Hermes Agent Driver & Supervised Antigravity Dogfood Execution)
.planning/phases/42-hermes-driver/42-VERIFICATION.md:9:  1. `crates/devflow-core/src/agents/hermes.rs`: Modular `HermesDriver` implementing `AgentDriver` with headless `-z` oneshot launch, `HERMES_ACCEPT_HOOKS=1`, claude-style prompt rendering, dynamic `hermes tools list` delegation probe, and presence-only health check.
.planning/phases/42-hermes-driver/42-VERIFICATION.md:10:  2. `crates/devflow-core/src/state.rs`: Registered `AgentKind::Hermes` variant, serde roundtrip, case-insensitive parser, and Display formatting.
.planning/phases/42-hermes-driver/42-VERIFICATION.md:11:  3. `crates/devflow-core/src/agents/mod.rs`: Driver dispatch wiring and 6-driver conformance suite enrollment (`every_driver_passes_the_conformance_suite` and `hermes_conformance_enrollment`).
.planning/phases/42-hermes-driver/42-VERIFICATION.md:12:  4. `crates/devflow-cli/src/commands.rs`: Added `hermes` cmd_check in `doctor_checks()` and `doctor_includes_hermes_check_in_the_seam` unit test.
.planning/phases/42-hermes-driver/42-VERIFICATION.md:13:  5. `crates/devflow-cli/tests/phase7_cli.rs`: Integration regressions (`hermes_marker_less_run_does_not_advance`, `hermes_nonzero_exit_does_not_advance`, `hermes_hung_process_is_detected_not_left_running`) with `MonitorReapGuard`.
.planning/phases/42-hermes-driver/42-VERIFICATION.md:14:  6. `crates/devflow-cli/src/preflight.rs`: Unlocked `--mode auto` for Antigravity in `unattended_launch_shape_condition` following successful supervised dogfood run.
.planning/phases/42-hermes-driver/42-VERIFICATION.md:33:| Hermes Unit Tests | `cargo test -p devflow-core --lib hermes` | 14 passed; 0 failed |
.planning/phases/42-hermes-driver/42-VERIFICATION.md:34:| AgentKind Tests | `cargo test -p devflow-core --lib agent_kind_hermes` | 5 passed; 0 failed |
.planning/phases/42-hermes-driver/42-VERIFICATION.md:35:| Conformance Suite | `cargo test -p devflow-core --lib hermes_conformance_enrollment` | 1 passed (6 drivers passing); 0 failed |
.planning/phases/42-hermes-driver/42-VERIFICATION.md:36:| Doctor Check | `cargo test -p devflow --bin devflow doctor_includes_hermes` | 1 passed; 0 failed |
.planning/phases/42-hermes-driver/42-VERIFICATION.md:37:| Transport Integration | `cargo test -p devflow --test phase7_cli hermes` | 3 passed; 0 failed |
.planning/phases/42-hermes-driver/42-VERIFICATION.md:38:| Preflight C2 Unlock | `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity` | 1 passed; 0 failed |

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "nl -ba crates/devflow-cli/src/preflight.rs | sed -n '60,115p;960,1010p;1100,1145p;3640,3745p'; nl -ba crates/devflow-cli/src/commands.rs | sed -n '560,700p'; nl -ba crates/devflow-cli/tests/phase7_cli.rs | sed -n '150,210p;1500,1585p;760,930p'" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 97ms:
    60	/// containing a separator, a PATH scan otherwise. Restores the fail-fast
    61	/// "is it installed?" diagnosis (14-CR-05) that the deleted synchronous
    62	/// launch path used to get from `ErrorKind::NotFound`: the monitor's `sh`
    63	/// exec of a missing binary only surfaces as a cryptic exit 127 after
    64	/// worktrees and monitors were already set up.
    65	fn agent_binary_available(program: &str) -> bool {
    66	    use std::os::unix::fs::PermissionsExt;
    67	    let executable = |path: &Path| {
    68	        path.is_file()
    69	            && std::fs::metadata(path)
    70	                .map(|m| m.permissions().mode() & 0o111 != 0)
    71	                .unwrap_or(false)
    72	    };
    73	    if program.contains('/') {
    74	        return executable(Path::new(program));
    75	    }
    76	    std::env::var_os("PATH")
    77	        .map(|paths| std::env::split_paths(&paths).any(|dir| executable(&dir.join(program))))
    78	        .unwrap_or(false)
    79	}
    80	
    81	/// The executable an agent kind launches, for preflighting before any
    82	/// scaffolding. The prompt/roots passed here are throwaways — adapters
    83	/// return a static program name regardless.
    84	pub(crate) fn agent_program(agent: AgentKind) -> &'static str {
    85	    agents::driver_for(agent)
    86	        .build_command(PhaseId::new(0), "", &[])
    87	        .0
    88	}
    89	
    90	pub(crate) fn ensure_agent_binary(program: &str) -> Result<(), CliError> {
    91	    if agent_binary_available(program) {
    92	        return Ok(());
    93	    }
    94	    Err(CliError::Message(format!(
    95	        "agent binary `{program}` not found — is it installed? (run `devflow doctor`)"
    96	    )))
    97	}
    98	
    99	// ---------------------------------------------------------------------------
   100	// 23f (gap closure, 23-12): phase-reachability guard. `devflow start` forks
   101	// a worktree (or, in `--no-worktree` mode, a feature branch) from `develop`
   102	// — if the target phase's ROADMAP.md heading or `.planning/phases/NN-*/`
   103	// directory is not actually present on `develop`, the run is invisible to
   104	// itself and floundered silently through Define before finally aborting
   105	// (the 2026-07-26 acceptance-run failure, `23-FINDINGS.md` §B1). This
   106	// probe catches that class of precondition failure BEFORE any git mutation.
   107	// ---------------------------------------------------------------------------
   108	
   109	/// The result of probing whether a phase is reachable from a base branch:
   110	/// both the ROADMAP heading and the phase directory must be present for
   111	/// `Reachable`; either being present-and-checkable but absent yields
   112	/// `Unreachable`, whose two fields record whether each half was **found**
   113	/// (at least one is always `false` in that variant). `Undeterminable` means
   114	/// the probe could not see the base branch at all (no such branch, no
   115	/// repository, or no `.planning/ROADMAP.md` on it) — the guard fails open
   960	/// a real dogfooded run (round-3 F5/D-04).
   961	///
   962	/// The chain-flag guard lives in a Rust process's stack frame; the legacy arm
   963	/// is a detached shell script with no frame to hang a `Drop` on
   964	/// (`35.1-RESEARCH.md` Pitfall 4). This turns that accepted gap from a silent
   965	/// stall hours into an unattended run into a refusal before the first agent is
   966	/// spawned.
   967	///
   968	/// Asks [`stream_launch_enabled`] — the launch path's OWN predicate —
   969	/// rather than re-deriving the answer, so the preflight and the launch cannot
   970	/// disagree about whether a guard will exist. `Stage::Code` is passed
   971	/// explicitly rather than `state.stage`: the question is about the stage where
   972	/// the mechanism is USED, not about the stage being launched right now.
   973	///
   974	/// **The dogfood term is the round-3 decision (F5/D-04).** Widening
   975	/// [`stream_launch_enabled`] to Antigravity (Task 3) would otherwise flip this
   976	/// condition to `Holds` and silently permit `--mode auto` for an UNDOGFOODED
   977	/// driver. Unattended mode stays refused for Antigravity until it has a real
   978	/// dogfooded run (tracked as a follow-up; the round-3 plan does not dogfood);
   979	/// Claude is the only stream agent with one today. Antigravity joins by
   980	/// replacing `state.agent == AgentKind::Claude` with an explicit dogfood flag.
   981	fn unattended_launch_shape_condition(state: &State) -> ConditionState {
   982	    if stream_launch_enabled(state.agent, Stage::Code, state.legacy_claude_launch)
   983	        && (state.agent == AgentKind::Claude || state.agent == AgentKind::Antigravity)
   984	    {
   985	        return ConditionState::Holds;
   986	    }
   987	    let mut causes = Vec::new();
   988	    if state.legacy_claude_launch {
   989	        causes.push("the legacy launch opt-out is active".to_string());
   990	    }
   991	    if state.agent != AgentKind::Claude && state.agent != AgentKind::Antigravity {
   992	        causes.push(format!(
   993	            "the agent is `{}`, not claude or antigravity",
   994	            state.agent
   995	        ));
   996	    }
   997	    if causes.is_empty() {
   998	        // Defensive: reachable only if `STREAM_JSON_STAGES` ever narrows to
   999	        // exclude Code. Reporting "no cause" would be worse than saying so.
  1000	        causes.push("Code is not on the stream-json launch path".to_string());
  1001	    }
  1002	    // The reason names the CAUSE that applied, and then the consequence in
  1003	    // terms true of every cause.
  1004	    ConditionState::DoesNotHold(format!(
  1005	        "{} — the chain-flag guard binds only inside the pipe-owning monitor, which this \
  1006	         launch shape never starts",
  1007	        causes.join(" and ")
  1008	    ))
  1009	}
  1010	
  1100	///
  1101	/// A write failure on the sink is deliberately IGNORED rather than propagated:
  1102	/// a closed stdout must not convert a viable launch into a refusal, nor a
  1103	/// refusal into a pass. The disposition is computed from the conditions, never
  1104	/// from whether the report reached anyone.
  1105	fn unattended_launch_check_reporting_to(
  1106	    project_root: &Path,
  1107	    state: &State,
  1108	    report: &mut dyn std::io::Write,
  1109	) -> Result<(), String> {
  1110	    if !unattended_launch_check_applies(state.stage) {
  1111	        return Ok(());
  1112	    }
  1113	    // The same spelling the chain-flag guard and the force-clear use: in
  1114	    // worktree mode the tracked `.planning/` the agent actually reads is the
  1115	    // WORKTREE's copy, not the main checkout's (999.76).
  1116	    let launch_root = state.worktree_path.as_deref().unwrap_or(project_root);
  1117	
  1118	    let conditions = [
  1119	        (
  1120	            "GSD config can hold the chain flag",
  1121	            unattended_config_condition(launch_root),
  1122	        ),
  1123	        (
  1124	            "Code would launch on the pipe-owning arm",
  1125	            unattended_launch_shape_condition(state),
  1126	        ),
  1127	        (
  1128	            "no plan declares a human-only checkpoint",
  1129	            unattended_planned_checkpoint_condition(launch_root, state),
  1130	        ),
  1131	    ];
  1132	
  1133	    // UNCONDITIONAL, and above the mode branch below on purpose (D-08).
  1134	    let _ = writeln!(
  1135	        report,
  1136	        "unattended-launch prerequisites — phase {phase}, stage {stage}, mode {mode}:",
  1137	        phase = state.phase,
  1138	        stage = state.stage,
  1139	        mode = state.mode
  1140	    );
  1141	    for (name, condition) in &conditions {
  1142	        let _ = match condition.detail() {
  1143	            Some(detail) => writeln!(report, "  [{}] {name} — {detail}", condition.label()),
  1144	            None => writeln!(report, "  [{}] {name}", condition.label()),
  1145	        };
  3640	    /// T-35.1-16: no reason may embed the absolute config path. The refusal
  3641	    /// string reaches a persisted gate file and the operator's notification.
  3642	    #[test]
  3643	    fn unattended_refusal_reason_contains_no_absolute_path() {
  3644	        let (dir, state) = viable_unattended_fixture(PhaseId::new(82));
  3645	        std::fs::remove_file(dir.path().join(".planning/config.json")).unwrap();
  3646	        let fixture_root = dir.path().to_string_lossy().into_owned();
  3647	
  3648	        let err = preflight_unattended_launch_check(dir.path(), &state).unwrap_err();
  3649	        assert!(!err.contains(&fixture_root), "{err}");
  3650	        assert!(!err.contains("/home/"), "{err}");
  3651	        assert!(!err.contains("/Users/"), "{err}");
  3652	        assert!(!err.contains("/tmp/"), "{err}");
  3653	    }
  3654	
  3655	    // ------------------------------------------------------------------
  3656	    // Phase 42 Task 3 (ANTG-04, D-07): unattended `--mode auto` holds for
  3657	    // Antigravity following successful supervised dogfood run.
  3658	    // ------------------------------------------------------------------
  3659	
  3660	    #[test]
  3661	    fn unattended_launch_shape_condition_antigravity_allowed() {
  3662	        let dir = tempfile::tempdir().unwrap();
  3663	        let state = State::new(
  3664	            PhaseId::new(42),
  3665	            AgentKind::Antigravity,
  3666	            Mode::Auto,
  3667	            dir.path().to_path_buf(),
  3668	        );
  3669	
  3670	        let condition = unattended_launch_shape_condition(&state);
  3671	        assert!(
  3672	            matches!(condition, ConditionState::Holds),
  3673	            "antigravity --mode auto must HOLD following dogfooding, got {condition:?}"
  3674	        );
  3675	    }
  3676	
  3677	    #[test]
  3678	    fn unattended_launch_shape_condition_claude_unchanged() {
  3679	        let dir = tempfile::tempdir().unwrap();
  3680	        let mut state = State::new(
  3681	            PhaseId::new(41),
  3682	            AgentKind::Claude,
  3683	            Mode::Auto,
  3684	            dir.path().to_path_buf(),
  3685	        );
  3686	
  3687	        // Claude on the stream path holds, exactly as before the widening.
  3688	        assert!(
  3689	            matches!(
  3690	                unattended_launch_shape_condition(&state),
  3691	                ConditionState::Holds
  3692	            ),
  3693	            "claude must hold on the stream path"
  3694	        );
  3695	
  3696	        // Claude under the legacy opt-out still refuses with the legacy cause.
  3697	        state.legacy_claude_launch = true;
  3698	        let ConditionState::DoesNotHold(reason) = unattended_launch_shape_condition(&state) else {
  3699	            panic!("claude + legacy opt-out must refuse");
  3700	        };
  3701	        assert!(
  3702	            reason.contains("legacy launch opt-out"),
  3703	            "the legacy cause must survive: {reason}"
  3704	        );
  3705	    }
  3706	
  3707	    #[test]
  3708	    fn unattended_launch_shape_condition_non_stream_agent_refused() {
  3709	        let dir = tempfile::tempdir().unwrap();
  3710	        let state = State::new(
  3711	            PhaseId::new(42),
  3712	            AgentKind::Pi,
  3713	            Mode::Auto,
  3714	            dir.path().to_path_buf(),
  3715	        );
  3716	
  3717	        let ConditionState::DoesNotHold(reason) = unattended_launch_shape_condition(&state) else {
  3718	            panic!("pi --mode auto must be refused");
  3719	        };
  3720	        assert!(
  3721	            reason.contains("not claude or antigravity"),
  3722	            "the refusal must name the non-stream cause: {reason}"
  3723	        );
  3724	        assert_eq!(
  3725	            agent_program(AgentKind::Antigravity),
  3726	            "agy",
  3727	            "the driver still resolves the agy program (unchanged path)"
  3728	        );
  3729	    }
  3730	}
   560	                "reference exists at {} (use --refresh to update it)",
   561	                path.display()
   562	            );
   563	            return Ok(());
   564	        }
   565	        worktree::remove(project_root, &path, true)?;
   566	        worktree::add_detached(project_root, &path, &branch)?;
   567	        println!(
   568	            "refreshed reference worktree at {} (snapshot of {branch})",
   569	            path.display()
   570	        );
   571	    } else {
   572	        worktree::add_detached(project_root, &path, &branch)?;
   573	        println!(
   574	            "created reference worktree at {} (snapshot of {branch})",
   575	            path.display()
   576	        );
   577	    }
   578	    Ok(())
   579	}
   580	
   581	/// Parse the phase number encoded in a `.worktrees/phase-NN[-agent]` path.
   582	/// Used only as a fallback join key when no persisted `State.worktree_path`
   583	/// matches the worktree entry (review: Codex MEDIUM — worktree->phase join).
   584	/// Returns `None` for paths that don't follow this naming (e.g. the static
   585	/// `reference` worktree), which correctly excludes it from the liveness
   586	/// guard — a snapshot has no owning phase/agent to be alive.
   587	fn phase_from_worktree_path(worktrees_dir: &Path, path: &Path) -> Option<PhaseId> {
   588	    let name = path.strip_prefix(worktrees_dir).ok()?.to_str()?;
   589	    let rest = name.strip_prefix("phase-")?;
   590	    // A dot is part of the identifier (`phase-35.1`), so it must be consumed
   591	    // here; stopping at the first non-digit would read `phase-35.1` as phase
   592	    // 35 and join a decimal phase's worktree to its integer sibling's state.
   593	    // The agent suffix (`phase-07-claude`) still terminates the run.
   594	    let label: String = rest
   595	        .chars()
   596	        .take_while(|c| c.is_ascii_digit() || *c == '.')
   597	        .collect();
   598	    label.parse().ok()
   599	}
   600	
   601	/// Join a `git worktree list` entry to its owning phase `State`, preferring
   602	/// the persisted `worktree_path` (set by `start`/`parallel`) and falling back
   603	/// to worktree-directory-name or branch-name matching only when no
   604	/// `worktree_path` match exists (review: Codex MEDIUM). Returns `None` when
   605	/// no owning state can be found at all (e.g. the phase already shipped and
   606	/// its state was cleared) — callers treat that as "no liveness signal",
   607	/// not as an implicit "safe to remove."
   608	fn state_for_worktree<'a>(
   609	    states: &'a [State],
   610	    worktrees_dir: &Path,
   611	    wt: &worktree::WorktreeInfo,
   612	) -> Option<&'a State> {
   613	    if let Some(state) = states
   614	        .iter()
   615	        .find(|s| s.worktree_path.as_deref() == Some(wt.path.as_path()))
   616	    {
   617	        return Some(state);
   618	    }
   619	    if let Some(phase) = phase_from_worktree_path(worktrees_dir, &wt.path)
   620	        && let Some(state) = states.iter().find(|s| s.phase == phase)
   621	    {
   622	        return Some(state);
   623	    }
   624	    if let Some(branch) = &wt.branch {
   625	        return states
   626	            .iter()
   627	            .find(|s| *branch == format!("{FEATURE_PREFIX}phase-{}", s.phase.padded()));
   628	    }
   629	    None
   630	}
   631	
   632	/// Bounded-backoff retry around `worktree::remove`, absorbing the transient
   633	/// `Directory not empty` race that can occur even after a phase is confirmed
   634	/// dead (a lingering fd/writer from the just-exited agent). NOT a substitute
   635	/// for the liveness guard above — only reached once a phase is confirmed
   636	/// dead (agent dead AND monitor not active). `git worktree prune` is
   637	/// deliberately not used here: it only clears metadata for already-absent
   638	/// directories and would orphan leftover files on disk (Pitfall 3).
   639	fn remove_worktree_with_retry(
   640	    project_root: &Path,
   641	    path: &Path,
   642	    force: bool,
   643	) -> Result<(), worktree::WorktreeError> {
   644	    const ATTEMPTS: u32 = 3;
   645	    const BASE_DELAY_MS: u64 = 50;
   646	    let mut last_err = None;
   647	    for attempt in 0..ATTEMPTS {
   648	        match worktree::remove(project_root, path, force) {
   649	            Ok(()) => return Ok(()),
   650	            Err(err) => {
   651	                last_err = Some(err);
   652	                if attempt + 1 < ATTEMPTS {
   653	                    std::thread::sleep(std::time::Duration::from_millis(
   654	                        BASE_DELAY_MS * 2u64.pow(attempt),
   655	                    ));
   656	                }
   657	            }
   658	        }
   659	    }
   660	    Err(last_err.expect("loop runs ATTEMPTS >= 1 times"))
   661	}
   662	
   663	/// Remove phase worktrees (and the reference with --force), deleting their
   664	/// associated feature branches, then prune and clean up merged branches.
   665	///
   666	/// Hard-refuses (D-06, no override flag) removal of any worktree whose owning
   667	/// phase has a live agent (any monitor state, including Unknown/Stuck) or an
   668	/// active monitor (Healthy/BetweenStages) — closing the race where a real
   669	/// `cleanup --force` run could delete a worktree a live agent/monitor is
   670	/// still writing into (review: Codex HIGH, fail-closed on a live agent).
   671	pub(crate) fn cleanup(project_root: &Path, force: bool) -> Result<(), CliError> {
   672	    let git = GitFlow::new(project_root);
   673	    let worktrees_dir = worktree::worktrees_dir(project_root);
   674	    let reference = worktree::reference_path(project_root);
   675	    let states = workflow::list_states(project_root);
   676	
   677	    let worktrees = worktree::list(project_root)?;
   678	    let mut removed = 0usize;
   679	    for wt in &worktrees {
   680	        // Only touch worktrees under `.worktrees/` (never the main checkout).
   681	        if !wt.path.starts_with(&worktrees_dir) {
   682	            continue;
   683	        }
   684	        if wt.path == reference && !force {
   685	            println!("keeping reference worktree (use --force to remove it)");
   686	            continue;
   687	        }
   688	
   689	        let matched_state = state_for_worktree(&states, &worktrees_dir, wt);
   690	        let phase = matched_state
   691	            .map(|s| s.phase)
   692	            .or_else(|| phase_from_worktree_path(&worktrees_dir, &wt.path));
   693	        let agent_alive = phase
   694	            .and_then(|p| agent_pid_from_file(project_root, p))
   695	            .is_some_and(agent::agent_running);
   696	        let monitor_pid = matched_state.and_then(|s| s.monitor_pid);
   697	        let monitor_alive = monitor_pid.is_some_and(agent::agent_running);
   698	        let phase_liveness = liveness(monitor_pid, monitor_alive, agent_alive);
   699	
   700	        // A phase halted via `devflow start --until <stage>` (20c) clears
   150	    let output = command.output().expect("run devflow");
   151	    assert!(
   152	        output.status.success(),
   153	        "devflow {args:?} failed\nstdout:\n{}\nstderr:\n{}",
   154	        String::from_utf8_lossy(&output.stdout),
   155	        String::from_utf8_lossy(&output.stderr)
   156	    );
   157	    output
   158	}
   159	
   160	/// Suite-level registry of monitor PIDs the tests' `MonitorReapGuard`s are
   161	/// responsible for (41-02 Task 1, codex-4).
   162	///
   163	/// Populated by [`MonitorReapGuard::after_launch`] (the pid the guard will
   164	/// reap — the SETTLED-state monitor, not the first-stage one), and drained by
   165	/// the guard's `Drop` after the verified reap. The suite audit asserts the
   166	/// registry is EMPTY once every bound guard has dropped — so an empty
   167	/// registry means "every monitor a test was responsible for was verified
   168	/// reaped", which is the claim HYG-01 makes. An unguarded test registers
   169	/// nothing and is therefore not what the registry detects; what it does
   170	/// detect is a guard that bound but failed to reap (its pid stays registered
   171	/// and alive), which `registered_monitors_alive` proves it can see.
   172	static REGISTRY: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<u32>>> =
   173	    std::sync::OnceLock::new();
   174	
   175	fn registry() -> &'static std::sync::Mutex<std::collections::HashSet<u32>> {
   176	    REGISTRY.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
   177	}
   178	
   179	/// Count of bound-but-not-yet-dropped `MonitorReapGuard`s — the suite audit's
   180	/// ordering barrier: the audit waits for this to reach 0 so it cannot race a
   181	/// still-running test into a false empty.
   182	static ACTIVE_GUARDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
   183	
   184	/// Count of verified reaps performed by the suite's guards. The audit requires
   185	/// this to be > 0 so an empty registry cannot be a vacuous "nothing ran" pass.
   186	static REAPED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
   187	
   188	/// The suite-audit detection helper (codex-4): which registered PIDs are
   189	/// still alive? Pure over the registry so it can be unit-tested to FAIL
   190	/// against a deliberately-alive registered PID — the audit must be able to
   191	/// redden, or a per-test guard that can never be wrong proves nothing.
   192	fn registered_monitors_alive(registry: &std::collections::HashSet<u32>) -> Vec<u32> {
   193	    registry
   194	        .iter()
   195	        .copied()
   196	        .filter(|pid| devflow_core::agent::agent_running(*pid))
   197	        .collect()
   198	}
   199	
   200	fn wait_for(path: &Path) {
   201	    for _ in 0..200 {
   202	        if path.exists() {
   203	            return;
   204	        }
   205	        std::thread::sleep(Duration::from_millis(25));
   206	    }
   207	    panic!("timed out waiting for {}", path.display());
   208	}
   209	
   210	/// Wait until a monitor-written pid file exists AND holds a parseable pid,
   760	        .output()
   761	        .expect("run devflow");
   762	
   763	    assert!(output.status.success());
   764	    let stdout = String::from_utf8_lossy(&output.stdout);
   765	    assert!(
   766	        !stdout.contains("STOPS HERE"),
   767	        "no stop annotation must appear without --until\nstdout: {stdout}"
   768	    );
   769	}
   770	
   771	#[test]
   772	fn status_prints_cron_hint_when_cron_instructions_exist() {
   773	    let repo = tempfile::tempdir().unwrap();
   774	    let root = repo.path();
   775	    init_repo(root);
   776	    let instructions = devflow_core::ship::build_single_agent_cron_instructions(
   777	        root,
   778	        PhaseId::new(7),
   779	        "2026-06-18T15:45:30Z",
   780	    );
   781	    devflow_core::ship::write_cron_instructions(root, &instructions).unwrap();
   782	    let fake_bin = fake_bin_dir(&[]);
   783	
   784	    let output = run_devflow(root, &fake_bin.path, &["status"]);
   785	    let stdout = String::from_utf8_lossy(&output.stdout);
   786	
   787	    assert!(stdout.contains(&format!(
   788	        "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
   789	        root.display()
   790	    )));
   791	}
   792	
   793	#[test]
   794	fn reference_and_cleanup_worktree_cli_flow() {
   795	    let repo = tempfile::tempdir().unwrap();
   796	    let root = repo.path();
   797	    init_repo(root);
   798	    // 31-03: the Code stage now runs the D-15 delivery canary before it
   799	    // launches, and refuses outright unless a token DevFlow planted comes back
   800	    // inside a TOP-LEVEL `result` event. A fake CLI that cannot do that is,
   801	    // correctly, a CLI this pipeline will not run on — so the fixture models a
   802	    // CLI that DOES deliver rather than working around the guard.
   803	    //
   804	    // `read -r turn` takes exactly one line and returns: the monitor writes the
   805	    // user turn followed by a newline, and blocking on full EOF would hang
   806	    // against a pipe deliberately held open past the first turn. On the legacy
   807	    // stages stdin is `/dev/null`, so the read yields nothing and the ordinary
   808	    // marker branch runs, exactly as before this change.
   809	    let fake_bin = fake_bin_dir(&[(
   810	        "claude",
   811	        r#"#!/bin/sh
   812	read -r turn
   813	case "$turn" in
   814	  *DEVFLOW_DELIVERY_CANARY_*)
   815	    token=$(printf '%s' "$turn" | grep -o 'DEVFLOW_DELIVERY_CANARY_[0-9a-f]*' | head -1)
   816	    printf '{"type":"result","subtype":"success","is_error":false,"session_id":"s-fake","result":"%s"}\n' "$token"
   817	    ;;
   818	  *)
   819	    printf 'DEVFLOW_RESULT: {"status":"success"}\n'
   820	    ;;
   821	esac
   822	"#,
   823	    )]);
   824	
   825	    // reference — creates static snapshot
   826	    let out = run_devflow(root, &fake_bin.path, &["reference"]);
   827	    let stdout = String::from_utf8_lossy(&out.stdout);
   828	    assert!(stdout.contains("reference worktree"));
   829	    assert!(root.join(".worktrees/reference").is_dir());
   830	
   831	    // start --worktree — creates phase worktree
   832	    let out = run_devflow(
   833	        root,
   834	        &fake_bin.path,
   835	        &[
   836	            "start",
   837	            "--phase",
   838	            "8",
   839	            "--agent",
   840	            "claude",
   841	            "--mode",
   842	            "auto",
   843	            "--worktree",
   844	        ],
   845	    );
   846	    let stdout = String::from_utf8_lossy(&out.stdout);
   847	    assert!(stdout.contains("phase 8"));
   848	    assert!(
   849	        root.join(".worktrees/phase-08").is_dir(),
   850	        "worktree not created"
   851	    );
   852	
   853	    // status — lists active worktrees
   854	    let out = run_devflow(root, &fake_bin.path, &["status"]);
   855	    let stdout = String::from_utf8_lossy(&out.stdout);
   856	    assert!(
   857	        stdout.contains(".worktrees/reference"),
   858	        "status missing reference\n{stdout}"
   859	    );
   860	    assert!(
   861	        stdout.contains(".worktrees/phase-08"),
   862	        "status missing phase worktree\n{stdout}"
   863	    );
   864	
   865	    // 20b: cleanup now hard-refuses while a monitor is still actively
   866	    // driving the phase (Healthy/BetweenStages) — a real operator would
   867	    // resolve the phase before cleaning it up, not race the still-running
   868	    // monitor. This fixture's fake agent never produces real work, so
   869	    // Validate always loops back and forces a gate after
   870	    // MAX_CONSECUTIVE_FAILURES; abort it (note containing "abort" —
   871	    // gates.rs::GateAction::from_response) so the monitor clears state,
   872	    // then wait for that to land before invoking cleanup.
   873	    wait_for(&root.join(".devflow/gates/08-validate.json"));
   874	
   875	    // 31-03: reaching the Validate gate already implies the Code stage's
   876	    // delivery canary confirmed — but only implicitly. Asserted explicitly so
   877	    // that a future change which stops running the guard (e.g. narrowing
   878	    // `STREAM_JSON_STAGES`) shows up here instead of passing silently.
   879	    let events = fs::read_to_string(root.join(".devflow/events.jsonl")).unwrap_or_default();
   880	    assert!(
   881	        events.contains("claude_delivery_canary_confirmed"),
   882	        "the Code launch must have run the delivery canary and confirmed it\n{events}"
   883	    );
   884	
   885	    // HYG-01 (41-02 review finding): `start --phase 8` spawned a monitor that
   886	    // now blocks at the Validate gate. Bind a guard so the monitor is
   887	    // verified-reaped on EVERY exit path — the `gate reject` teardown below is
   888	    // the happy path, not a substitute for the guard.
   889	    let settled = load_state(root, PhaseId::new(8)).expect("load state");
   890	    let _reap = MonitorReapGuard::after_launch(&settled);
   891	
   892	    run_devflow(
   893	        root,
   894	        &fake_bin.path,
   895	        &[
   896	            "gate",
   897	            "reject",
   898	            "8",
   899	            "--stage",
   900	            "validate",
   901	            "--note",
   902	            "abort test teardown",
   903	        ],
   904	    );
   905	    wait_for_state_cleared(root, PhaseId::new(8));
   906	
   907	    // cleanup — removes worktrees
   908	    let out = run_devflow(root, &fake_bin.path, &["cleanup", "--force"]);
   909	    let stdout = String::from_utf8_lossy(&out.stdout);
   910	    assert!(stdout.contains("removed"));
   911	
   912	    // cleanup --force removes everything including reference
   913	    assert!(!root.join(".worktrees/reference").is_dir());
   914	    assert!(!root.join(".worktrees/phase-08").is_dir());
   915	}
   916	
   917	/// 13-06 dogfood regression (Codex leg): a fresh headless Codex run can
   918	/// never pass Define, so `start --agent codex` on a phase with no CONTEXT.md
   919	/// on develop must fail fast in pre-flight — before any worktree, branch, or
   920	/// monitor is created.
   921	#[test]
   922	fn start_codex_without_context_fails_preflight() {
   923	    let repo = tempfile::tempdir().unwrap();
   924	    let root = repo.path();
   925	    init_repo(root);
   926	    // codex IS installed (the 13-06 dogfood scenario) — the binary preflight
   927	    // (14-CR-05) passes and the CONTEXT.md artifact check must fire next.
   928	    let fake_bin = fake_bin_dir(&[("codex", "#!/bin/sh\nexit 0\n")]);
   929	
   930	    let output = Command::new(devflow_bin())
  1500	    format!("{SCHEMA_AND_CANARY}{body}")
  1501	}
  1502	
  1503	#[derive(Clone, Copy)]
  1504	enum StubMode {
  1505	    MarkerStream,
  1506	    Quiet,
  1507	    InitOnly,
  1508	}
  1509	
  1510	/// Reap the detached `__monitor` wrapper a `devflow start` run left behind
  1511	/// (phase 41 Task 8, antigravity notice (a)): the integration-suite analogue
  1512	/// of the binary crate's `ReapMonitorOnDrop`, built on the PUBLIC
  1513	/// `devflow_core::agent` surface (integration tests cannot reach
  1514	/// `devflow-cli`'s test_support). TERM->KILL escalation with VERIFIED death,
  1515	/// keyed to `state.monitor_pid`, bound strictly after the final
  1516	/// `&mut State` use. 41-02 Task 1 turns this into the systematic pass.
  1517	struct MonitorReapGuard {
  1518	    pid: Option<u32>,
  1519	}
  1520	
  1521	impl MonitorReapGuard {
  1522	    fn after_launch(state: &devflow_core::state::State) -> Self {
  1523	        // Register the pid THIS guard is responsible for (the settled-state
  1524	        // monitor it will reap), and mark a guard in flight for the audit's
  1525	        // ordering barrier. `state` must be the SETTLED state
  1526	        // (`wait_for_settled` / `wait_for_gate`), whose `monitor_pid` is the
  1527	        // chain's LAST monitor — the one that would leak if not reaped.
  1528	        if let Some(pid) = state.monitor_pid {
  1529	            registry().lock().unwrap().insert(pid);
  1530	            ACTIVE_GUARDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
  1531	        }
  1532	        Self {
  1533	            pid: state.monitor_pid,
  1534	        }
  1535	    }
  1536	}
  1537	
  1538	impl Drop for MonitorReapGuard {
  1539	    fn drop(&mut self) {
  1540	        let Some(pid) = self.pid else {
  1541	            return;
  1542	        };
  1543	        devflow_core::agent::terminate_and_verify(
  1544	            pid,
  1545	            devflow_core::agent::TERMINATE_VERIFY_WAIT,
  1546	            devflow_core::agent::TERMINATE_VERIFY_POLL,
  1547	        );
  1548	        // Deregister after the verified reap: the suite audit's empty-registry
  1549	        // assertion then means "every monitor a guard was responsible for was
  1550	        // verified reaped".
  1551	        registry().lock().unwrap().remove(&pid);
  1552	        // Ordering-barrier bookkeeping, regardless of the reap verdict: the
  1553	        // guard is no longer in flight, and a reap was attempted.
  1554	        ACTIVE_GUARDS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
  1555	        REAPED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
  1556	        if devflow_core::agent::agent_running(pid) {
  1557	            if std::thread::panicking() {
  1558	                use std::io::Write as _;
  1559	                let _ = writeln!(
  1560	                    std::io::stderr(),
  1561	                    "MonitorReapGuard: monitor wrapper pid {pid} still alive after reap \
  1562	                     during an unwind — not re-panicking because a panic is already in flight"
  1563	                );
  1564	            } else {
  1565	                panic!(
  1566	                    "monitor wrapper pid {pid}, spawned by this test's own start run, must be \
  1567	                     verified dead after reaping — not merely assumed dead"
  1568	                );
  1569	            }
  1570	        }
  1571	    }
  1572	}
  1573	
  1574	/// ANTG-03: a stubbed `agy` that exits 0 with no stream events must not
  1575	/// advance a COMMIT-GATED stage. Define (not commit-gated) legitimately
  1576	/// advances on exit 0; Plan — a commit-gated stage — produces no marker and
  1577	/// no commits, so it gates instead of advancing to Code.
  1578	#[test]
  1579	fn marker_less_antigravity_never_advances() {
  1580	    let repo = tempfile::tempdir().unwrap();
  1581	    let root = repo.path();
  1582	    init_repo(root);
  1583	    let fake_bin = fake_bin_dir(&[("agy", &antigravity_stub(StubMode::Quiet))]);
  1584	
  1585	    run_devflow(

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc 'git diff -- crates/devflow-core/src/agents/hermes.rs crates/devflow-core/src/agents/mod.rs crates/devflow-core/src/state.rs crates/devflow-cli/src/commands.rs crates/devflow-cli/src/preflight.rs crates/devflow-cli/tests/phase7_cli.rs' in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 0ms:
hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "rg -n \"\\.environment\\(\\)|build_command\\(|Command::new|envs|env\\(\" crates/devflow-cli crates/devflow-core/src | head -200" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 274ms:
crates/devflow-cli/tests/yes_ship_config.rs:69:    let output = Command::new(devflow_bin())
crates/devflow-core/src/worktree.rs:366:    /// A literal chained `.env("GIT_DIR", foreign)`-after-the-constructor
crates/devflow-core/src/worktree.rs:411:            cmd.get_envs()
crates/devflow-cli/tests/stop_e2e.rs:11://! `Command::new`, not an in-process call.
crates/devflow-cli/tests/stop_e2e.rs:27:/// `Command::new("git")`.
crates/devflow-cli/tests/stop_e2e.rs:147:    // return promptly. Set per-`Command` via `.env(...)`, never via a
crates/devflow-cli/tests/stop_e2e.rs:149:    let mut child = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:152:        .env("DEVFLOW_GATE_TIMEOUT_SECS", "15")
crates/devflow-cli/tests/stop_e2e.rs:177:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:233:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:269:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:301:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:316:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:342:        Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:396:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:423:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:474:    let stopped = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:492:    let refused = Command::new(devflow_bin())
crates/devflow-cli/tests/stop_e2e.rs:509:    let forced = Command::new(devflow_bin())
crates/devflow-core/src/version.rs:2422:    /// this plan's migration, both bare `Command::new("git")` sites this
crates/devflow-core/src/version.rs:2477:        let out = std::process::Command::new(&exe)
crates/devflow-core/src/version.rs:2485:            .env(INNER_ROOT, root.to_str().unwrap())
crates/devflow-core/src/version.rs:2486:            .env("GIT_DIR", foreign.path().join(".git"))
crates/devflow-core/src/phase_id.rs:435:        let probe = std::process::Command::new(&gsd_tools)
crates/devflow-core/src/verify.rs:229:    std::process::Command::new("sh")
crates/devflow-cli/tests/start_reachability_e2e.rs:23:/// `Command::new("git")`.
crates/devflow-cli/tests/start_reachability_e2e.rs:123:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/start_reachability_e2e.rs:128:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/start_reachability_e2e.rs:193:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/start_reachability_e2e.rs:205:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-core/src/hooks.rs:231:    let output = Command::new("sh")
crates/devflow-core/src/hooks.rs:350:    Command::new("date")
crates/devflow-core/src/test_support.rs:200:        let mut child = std::process::Command::new("sleep")
crates/devflow-cli/tests/release_check.rs:23:    Command::new(devflow_bin())
crates/devflow-cli/tests/release_check.rs:27:        .env("HOME", isolated_home.path())
crates/devflow-core/src/gsd_config.rs:512:    /// `Command::new(\"git\")`.
crates/devflow-cli/tests/doctor_antigravity.rs:40:    let out = Command::new(devflow_bin())
crates/devflow-cli/tests/doctor_antigravity.rs:43:        .env("PATH", &bin)
crates/devflow-cli/tests/doctor_antigravity.rs:67:    let out = Command::new(devflow_bin())
crates/devflow-cli/tests/doctor_antigravity.rs:70:        .env("PATH", &empty)
crates/devflow-core/src/monitor.rs:300:    envs: &[(String, String)],
crates/devflow-core/src/monitor.rs:303:    spawn_monitor_inner(state, program, args, envs, launch, true)
crates/devflow-core/src/monitor.rs:310:    envs: &[(String, String)],
crates/devflow-core/src/monitor.rs:364:        // `.envs(...)` on the `__monitor` process below), and that is only
crates/devflow-core/src/monitor.rs:374:        if !envs.is_empty() {
crates/devflow-core/src/monitor.rs:379:                envs.len()
crates/devflow-core/src/monitor.rs:421:        // construction and `.envs(...)` runs after, so deliberate
crates/devflow-core/src/monitor.rs:440:            .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
crates/devflow-core/src/monitor.rs:498:    // `Command::new("sh")`. This is the spawn that launches the coding agent
crates/devflow-core/src/monitor.rs:507:    // construction, and `.envs(...)` below runs after, so an adapter that
crates/devflow-core/src/monitor.rs:519:        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
crates/devflow-core/src/monitor.rs:867:    envs: &[(String, String)],
crates/devflow-core/src/monitor.rs:891:        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
crates/devflow-core/src/monitor.rs:2532:    /// A bare `Command::new("git")` here would itself inherit an ambient
crates/devflow-core/src/monitor.rs:2622:        let out = std::process::Command::new(&exe)
crates/devflow-core/src/monitor.rs:2629:            .env(INNER_ROOT, root.to_str().unwrap())
crates/devflow-core/src/monitor.rs:2630:            .env("GIT_DIR", foreign.path().join(".git"))
crates/devflow-cli/tests/reap_strays_e2e.rs:97:    let child = std::process::Command::new("sh")
crates/devflow-cli/tests/reap_strays_e2e.rs:162:    let stop_output = Command::new(devflow_bin())
crates/devflow-cli/tests/reap_strays_e2e.rs:234:    let mut child = Command::new("sh")
crates/devflow-core/src/git.rs:65:/// `Command::new("git")` directly. `GIT_EXEC_PATH` is deliberately left
crates/devflow-core/src/git.rs:88:    let mut cmd = Command::new(program);
crates/devflow-core/src/git.rs:463:            .env("LC_ALL", "C")
crates/devflow-core/src/git.rs:464:            .env("LANG", "C")
crates/devflow-core/src/git.rs:490:            .env("LC_ALL", "C")
crates/devflow-core/src/git.rs:491:            .env("LANG", "C")
crates/devflow-core/src/git.rs:1541:            .env("GIT_DIR", foreign_repo.path().join(".git"))
crates/devflow-core/src/git.rs:1570:    /// unscrubbed `Command::new("git")` reproduction chaining a hostile
crates/devflow-core/src/git.rs:1571:    /// `.env("GIT_DIR", foreign)` on top was deliberately NOT added here:
crates/devflow-core/src/git.rs:1593:            cmd.get_envs()
crates/devflow-core/src/git.rs:1613:            .get_envs()
crates/devflow-core/src/git.rs:1634:            !cmd.get_envs()
crates/devflow-cli/tests/phase7_cli.rs:131:    let mut command = Command::new(devflow_bin());
crates/devflow-cli/tests/phase7_cli.rs:135:        .env("PATH", path_with_fake_bin(fake_bin))
crates/devflow-cli/tests/phase7_cli.rs:136:        .env("DEVFLOW_TEST_ROOT", root)
crates/devflow-cli/tests/phase7_cli.rs:139:        command.env("DEVFLOW_CLAUDE_LEGACY_LAUNCH", "true");
crates/devflow-cli/tests/phase7_cli.rs:149:    command.env("DEVFLOW_GATE_TIMEOUT_SECS", "60");
crates/devflow-cli/tests/phase7_cli.rs:626:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:631:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:664:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:669:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:696:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:710:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:746:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:758:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:930:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:935:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:1000:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:1003:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:1065:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:1068:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:1131:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:1134:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:1188:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:1191:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:1236:    let first = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:1239:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:1250:    let second = Command::new(devflow_bin())
crates/devflow-cli/tests/phase7_cli.rs:1253:        .env("PATH", path_with_fake_bin(&fake_bin.path))
crates/devflow-cli/tests/phase7_cli.rs:1413:        Command::new("kill")
crates/devflow-cli/tests/phase7_cli.rs:1430:        Command::new("kill")
crates/devflow-cli/tests/phase7_cli.rs:1901:        Command::new("kill")
crates/devflow-cli/tests/phase7_cli.rs:1917:        Command::new("kill")
crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs:40:/// `Command::new("git")`.
crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs:255:        let child = Command::new(devflow_bin())
crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs:301:        Command::new(devflow_bin())
crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs:306:            .env("PATH", git_only_path)
crates/devflow-cli/tests/auto_chain_leak_repair_e2e.rs:354:    let located = Command::new("sh")
crates/devflow-cli/src/test_support.rs:209:    fn build_command(
crates/devflow-cli/src/test_support.rs:248:    fn build_command(
crates/devflow-cli/src/test_support.rs:662:    /// `git_command` -> `hermetic_command` -> `Command::new("git")`), so this
crates/devflow-cli/src/test_support.rs:726:                Command::new("sleep")
crates/devflow-cli/src/staleness.rs:451:            .env("GIT_DIR", foreign_root.join(".git"))
crates/devflow-cli/src/staleness.rs:1021:    /// `Command::env()` call and nothing else — the literal "spawned child
crates/devflow-cli/src/staleness.rs:1026:    /// with no injection point of its own to chain `.env()` onto directly —
crates/devflow-cli/src/staleness.rs:1028:    /// git 2.55.0) chaining `.env("GIT_DIR", foreign)` directly onto a
crates/devflow-cli/src/staleness.rs:1074:        let status = std::process::Command::new(&exe)
crates/devflow-cli/src/staleness.rs:1077:            .env(INNER_ROOT, root.to_str().unwrap())
crates/devflow-cli/src/staleness.rs:1078:            .env(INNER_COMMIT, &base)
crates/devflow-cli/src/staleness.rs:1079:            .env("GIT_DIR", foreign_root.join(".git"))
crates/devflow-cli/src/preflight.rs:86:        .build_command(PhaseId::new(0), "", &[])
crates/devflow-cli/src/preflight.rs:658:    match std::process::Command::new("gh")
crates/devflow-cli/src/preflight.rs:2639:    /// literally chaining `.env("GIT_DIR", foreign)` onto this function's
crates/devflow-cli/src/preflight.rs:2652:    /// pre-migration `ls-tree` step — `std::process::Command::new("git")`
crates/devflow-cli/src/preflight.rs:2672:    /// shell is inherited by every unscrubbed `Command::new("git")` this
crates/devflow-cli/src/preflight.rs:2694:        // `Command::new("git")` spelling, so this deliberately-unscrubbed
crates/devflow-cli/src/preflight.rs:2697:        // sites (`rg -o 'Command::new\("git"\)'`).
crates/devflow-cli/src/preflight.rs:2699:        let vulnerable = std::process::Command::new(git_program)
crates/devflow-cli/src/preflight.rs:2709:            .env("GIT_DIR", foreign_root.join(".git"))
crates/devflow-cli/src/preflight.rs:3107:        let vulnerable = std::process::Command::new(git_program)
crates/devflow-cli/src/preflight.rs:3115:            .env("GIT_DIR", demo_foreign.path().join(".git"))
crates/devflow-cli/src/pipeline_launch.rs:178:        &driver.environment(),
crates/devflow-cli/src/pipeline_launch.rs:201:        let (program, args) = driver.build_command(phase, &prompt, roots);
crates/devflow-cli/src/pipeline_launch.rs:211:        let (program, args) = driver.build_command(phase, &prompt, roots);
crates/devflow-cli/src/pipeline_launch.rs:876:/// `envs` is deliberately empty here — the adapter's extra env was applied to
crates/devflow-cli/src/pipeline_launch.rs:1144:    let (program, _args) = driver.build_command(state.phase, &prompt, &roots);
crates/devflow-cli/src/main.rs:503:            let filter = tracing_subscriber::EnvFilter::try_from_default_env()
crates/devflow-cli/src/main.rs:512:            let filter = tracing_subscriber::EnvFilter::try_from_default_env()
crates/devflow-cli/src/commands.rs:2206:        match Command::new(cmd).arg(version_arg).output() {
crates/devflow-cli/src/commands.rs:4493:        let mut child = std::process::Command::new("sleep")
crates/devflow-cli/src/commands.rs:4523:        let mut child = std::process::Command::new("sleep")
crates/devflow-cli/src/commands.rs:4552:        let mut child = std::process::Command::new("sh")
crates/devflow-cli/src/commands.rs:4585:        let mut child = std::process::Command::new("sleep")
crates/devflow-cli/src/commands.rs:4623:        let mut child = std::process::Command::new("sh")
crates/devflow-cli/src/commands.rs:4664:        let mut child = std::process::Command::new("sh")
crates/devflow-cli/src/commands.rs:4757:        let mut child = std::process::Command::new("sh")
crates/devflow-cli/src/commands.rs:4803:        let mut child = std::process::Command::new("sh")
crates/devflow-cli/src/commands.rs:4965:        let mut child = std::process::Command::new("sleep")
crates/devflow-cli/src/commands.rs:5949:            let mut child = std::process::Command::new("sh")
crates/devflow-cli/src/commands.rs:6070:        let child = std::process::Command::new("sh")
crates/devflow-cli/src/commands.rs:6576:            let status = std::process::Command::new(&exe)
crates/devflow-cli/src/commands.rs:6579:                .env(INNER_ROOT, real_root.to_str().unwrap())
crates/devflow-cli/src/commands.rs:6580:                .env(INNER_TAG, "v1.7.0")
crates/devflow-cli/src/commands.rs:6581:                .env(INNER_BASE, "main")
crates/devflow-cli/src/commands.rs:6582:                .env("GIT_DIR", foreign.path().join(".git"))
crates/devflow-cli/tests/auto_chain_flag_e2e.rs:32:/// `Command::new("git")`.
crates/devflow-cli/tests/auto_chain_flag_e2e.rs:199:        Command::new(devflow_bin())
crates/devflow-cli/build.rs:77:    let output = Command::new("git")
crates/devflow-cli/tests/git_env_hermeticity.rs:30:    let output = Command::new("git")
crates/devflow-cli/tests/git_env_hermeticity.rs:49:fn suite_does_not_inherit_repo_local_git_env() {
crates/devflow-cli/tests/gate_sweep_e2e.rs:10://! `Command::new`, not an in-process call.
crates/devflow-cli/tests/gate_sweep_e2e.rs:26:/// `Command::new("git")`.
crates/devflow-cli/tests/gate_sweep_e2e.rs:176:        let output = Command::new(devflow_bin())
crates/devflow-cli/tests/gate_sweep_e2e.rs:216:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/gate_sweep_e2e.rs:246:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/gate_sweep_e2e.rs:272:/// OS process via `Command::new`, not driven in-process on a thread.
crates/devflow-cli/tests/gate_sweep_e2e.rs:290:    // Set per-`Command` via `.env(...)`, not via a process-global env
crates/devflow-cli/tests/gate_sweep_e2e.rs:292:    let mut child = Command::new(devflow_bin())
crates/devflow-cli/tests/gate_sweep_e2e.rs:295:        .env("DEVFLOW_GATE_TIMEOUT_SECS", "15")
crates/devflow-cli/tests/help_snapshot.rs:28:    let output = Command::new(devflow_bin())
crates/devflow-cli/tests/log_format_env.rs:61:    let mut cmd = Command::new(devflow_bin());
crates/devflow-cli/tests/log_format_env.rs:69:        cmd.env("DEVFLOW_LOG_FORMAT", fmt);
crates/devflow-cli/tests/log_format_env.rs:72:        cmd.env("RUST_LOG", level);
crates/devflow-cli/tests/log_format_env.rs:108:    // No RUST_LOG set — tracing-subscriber's EnvFilter::from_default_env()
crates/devflow-cli/tests/log_format_env.rs:129:/// `EnvFilter::from_default_env()`, which defaults to ERROR-only when
crates/devflow-cli/tests/log_format_env.rs:131:/// via `try_from_default_env().unwrap_or_else(...)` on both branches, so
crates/devflow-cli/tests/log_format_env.rs:138:/// commit 50db857 were reverted (back to the bare `from_default_env()`),
crates/devflow-cli/tests/log_format_env.rs:150:    let mut cmd = Command::new(devflow_bin());
crates/devflow-core/src/gates.rs:330:    let output = Command::new("sh")
crates/devflow-core/src/gates.rs:333:        .env("DEVFLOW_GATE_PHASE", phase.to_string())
crates/devflow-core/src/gates.rs:334:        .env("DEVFLOW_GATE_STAGE", stage.to_string())
crates/devflow-core/src/gates.rs:335:        .env("DEVFLOW_GATE_CONTEXT", context)
crates/devflow-core/src/gates.rs:336:        .env(
crates/devflow-core/src/canary.rs:296:        let (program, args) = ClaudeDriver.build_command(PhaseId::new(0), prompt, &[]);
crates/devflow-core/src/canary.rs:327:        let (program, args) = AntigravityDriver.build_command(PhaseId::new(0), prompt, &[]);
crates/devflow-core/src/canary.rs:518:    let output = std::process::Command::new("claude")
crates/devflow-core/src/canary.rs:534:    let output = std::process::Command::new("agy")
crates/devflow-core/src/agents/hermes.rs:35:    fn build_command(
crates/devflow-core/src/agents/hermes.rs:58:        let output = std::process::Command::new("hermes")
crates/devflow-core/src/agents/hermes.rs:76:        std::process::Command::new("hermes")
crates/devflow-core/src/agents/hermes.rs:121:    fn hermes_driver_build_command() {
crates/devflow-core/src/agents/hermes.rs:123:        let (prog, args) = driver.build_command(PhaseId::new(42), "test prompt", &[]);
crates/devflow-core/src/agents/hermes.rs:139:        let envs = driver.environment();
crates/devflow-core/src/agents/hermes.rs:140:        assert_eq!(envs, vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]);
crates/devflow-core/src/agents/pi.rs:54:    fn build_command(
crates/devflow-core/src/agents/pi.rs:81:        let output = std::process::Command::new("pi")
crates/devflow-core/src/agents/pi.rs:173:    let Ok(output) = std::process::Command::new("pi")
crates/devflow-core/src/agents/pi.rs:198:        let (program, args) = PiDriver.build_command(PhaseId::new(1), "do the thing", &[]);
crates/devflow-core/src/agents/mod.rs:83:    fn build_command(
crates/devflow-core/src/agents/mod.rs:165:    let (program, _args) = driver.build_command(PhaseId::new(1), "contract", &[]);

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "nl -ba crates/devflow-cli/src/pipeline_launch.rs | sed -n '130,235p;840,930p;1110,1170p'; nl -ba crates/devflow-core/src/monitor.rs | sed -n '280,460p;480,545p;850,915p'" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 0ms:
   130	    // ahead of `spawn_agent_and_record` so a refusal costs no archival rollover
   131	    // and spawns no monitor.
   132	    // Resolved into owned values BEFORE the gate takes `&mut State`. The
   133	    // canary's child runs where the stage's agent would (the worktree when
   134	    // there is one), and its throwaway capture lands beside the run's other
   135	    // runtime files — never on the phase capture the Layer 1 cascade reads.
   136	    let canary_workdir = state
   137	        .worktree_path
   138	        .as_deref()
   139	        .unwrap_or(&state.project_root)
   140	        .to_path_buf();
   141	    let canary_capture_dir = state.project_root.join(".devflow");
   142	    // D-07 (round-3 B2): the launcher is selected BY AGENT. The widened
   143	    // predicate arms the canary gate for Antigravity runs, and a hardcoded
   144	    // ClaudeCanaryLauncher there would spend a Claude invocation per
   145	    // Antigravity run — and refuse to launch when `claude` is absent or
   146	    // unauthenticated.
   147	    let canary_launcher = canary_launcher_for(state.agent, canary_workdir);
   148	    canary_gate(state, stream_launch, move || {
   149	        canary::run_delivery_canary(canary_launcher.as_ref(), &canary_capture_dir)
   150	    })?;
   151	
   152	    let (program, args, launch) = resolve_launch_shape(
   153	        state.agent,
   154	        driver.as_ref(),
   155	        state.phase,
   156	        prompt,
   157	        &roots,
   158	        stream_launch,
   159	    );
   160	
   161	    // 28-03 (D-03/D-04): every ORDINARY fresh stage launch starts the
   162	    // checkpoint-resume budget over, including a human-approved gate retry
   163	    // (which also routes through this function). Only `launch_stage_inner`
   164	    // resets this counter, and only `relaunch_checkpoint_session` increments
   165	    // it — that pairing is what makes `mode::MAX_CHECKPOINT_RESUMES` bound
   166	    // one stage's resume attempts, not a phase's entire lifetime (the same
   167	    // distinction `MAX_INFRA_FAILURES`'s doc comment draws for
   168	    // `infra_failures`). Persisted below by `spawn_agent_and_record`'s own
   169	    // `save_state` calls — no extra save needed here.
   170	    state.checkpoint_resumes = 0;
   171	
   172	    stamp_validate_dispatch_window(state);
   173	
   174	    spawn_agent_and_record(
   175	        state,
   176	        program,
   177	        &args,
   178	        &driver.environment(),
   179	        archived_stage,
   180	        launch,
   181	    )
   182	}
   183	
   184	/// Resolve a stage launch into `(program, argv, monitor arm)`.
   185	///
   186	/// Extracted from [`launch_stage_inner`] unchanged (31-04) so the shape a
   187	/// launch resolves to is assertable without spawning a process. The body is the
   188	/// pre-extraction `if/else if/else` verbatim; `stream_launch` is the caller's
   189	/// already-computed [`stream_launch_enabled`] reading, threaded in
   190	/// rather than recomputed so one predicate still governs the launch shape, the
   191	/// canary gate, and the D-11 notice.
   192	fn resolve_launch_shape(
   193	    agent: AgentKind,
   194	    driver: &dyn agents::AgentDriver,
   195	    phase: PhaseId,
   196	    prompt: String,
   197	    roots: &[std::path::PathBuf],
   198	    stream_launch: bool,
   199	) -> (&'static str, Vec<String>, monitor::MonitorLaunch) {
   200	    if stream_launch {
   201	        let (program, args) = driver.build_command(phase, &prompt, roots);
   202	        (program, args, monitor::MonitorLaunch::PipeOwning { prompt })
   203	    } else if agent == AgentKind::Claude {
   204	        // Claude on a stage the rollout has not reached, or a run that took
   205	        // D-11's opt-out: the explicitly named pre-31 builder, NOT
   206	        // `exec_command` — which now returns the stream-json shape for every
   207	        // stage.
   208	        let (program, args) = agents::ClaudeDriver::exec_command_single_document(&prompt);
   209	        (program, args, monitor::MonitorLaunch::Legacy)
   210	    } else {
   211	        let (program, args) = driver.build_command(phase, &prompt, roots);
   212	        (program, args, monitor::MonitorLaunch::Legacy)
   213	    }
   214	}
   215	
   216	/// Where a forced legacy launch's authorization came from, for the provenance
   217	/// record.
   218	#[derive(Debug, Clone, Copy, PartialEq, Eq)]
   219	enum LegacyLaunchSource {
   220	    /// `DEVFLOW_CLAUDE_LEGACY_LAUNCH` is set in THIS process's environment.
   221	    Environment,
   222	    /// The persisted `state.legacy_claude_launch`, written at `start`/`resume`
   223	    /// time by the `--legacy-claude-launch` flag or by the environment variable
   224	    /// as it stood then.
   225	    PersistedState,
   226	}
   227	
   228	impl LegacyLaunchSource {
   229	    fn as_str(self) -> &'static str {
   230	        match self {
   231	            LegacyLaunchSource::Environment => "env:DEVFLOW_CLAUDE_LEGACY_LAUNCH",
   232	            LegacyLaunchSource::PersistedState => "state:legacy_claude_launch",
   233	        }
   234	    }
   235	}
   840	                if changed {
   841	                    info!(
   842	                        "GSD chain flag set to {active} for this stage at {}",
   843	                        gsd_config::config_path(config_root).display()
   844	                    );
   845	                }
   846	            }
   847	            Err(err) => warn!(
   848	                "could not set the GSD chain flag at {}: {err} — proceeding without \
   849	                 checkpoint auto-approval",
   850	                gsd_config::config_path(config_root).display()
   851	            ),
   852	        }
   853	        Self {
   854	            config_root: config_root.to_path_buf(),
   855	        }
   856	    }
   857	}
   858	
   859	impl Drop for AutoChainGuard {
   860	    fn drop(&mut self) {
   861	        if let Err(err) = gsd_config::set_auto_chain_active(&self.config_root, false) {
   862	            warn!(
   863	                "could not clear the GSD chain flag at {}: {err}",
   864	                gsd_config::config_path(&self.config_root).display()
   865	            );
   866	        }
   867	    }
   868	}
   869	
   870	/// The detached pipe-owning monitor's own process body (Phase 31): supervise
   871	/// the child, then advance the stage machine exactly as the shell monitor's
   872	/// `devflow advance` tail did.
   873	///
   874	/// Runs in the `__monitor` process, never in the operator's CLI.
   875	///
   876	/// `envs` is deliberately empty here — the adapter's extra env was applied to
   877	/// THIS process by `spawn_monitor` and rides down by inheritance. That is
   878	/// sufficient only because the sole adapter routed through the pipe-owning arm
   879	/// (Claude) declares no extra env; see the note at `spawn_monitor`'s
   880	/// `PipeOwning` arm before widening it.
   881	pub(crate) fn run_monitor(
   882	    project_root: &Path,
   883	    phase: PhaseId,
   884	    workdir: &Path,
   885	    prompt_file: &Path,
   886	    idle_timeout_secs: u64,
   887	    agent: AgentKind,
   888	    argv: &[String],
   889	) -> Result<(), CliError> {
   890	    let prompt = std::fs::read_to_string(prompt_file).map_err(|err| {
   891	        CliError::Message(format!(
   892	            "monitor could not read the prompt file {}: {err}",
   893	            prompt_file.display()
   894	        ))
   895	    })?;
   896	    let Some((program, args)) = argv.split_first() else {
   897	        return Err(CliError::Message(
   898	            "monitor was given no child program to supervise".to_string(),
   899	        ));
   900	    };
   901	
   902	    // D-01/D-06: hold GSD's `workflow._auto_chain_active` at the value this
   903	    // launch requires for as long as the child runs, and return it to `false`
   904	    // when this function returns — by `?` on the monitor's `Err` OR by falling
   905	    // through to `advance` below. Both exits are covered because the guard is
   906	    // bound to a named variable in THIS scope; `let _ = ...` would drop it
   907	    // immediately and the flag's true-window would collapse to nothing.
   908	    //
   909	    // The target is `workdir`, not `project_root`: `.planning/config.json` is a
   910	    // tracked file inside the worktree the agent's cwd is set to, and that copy
   911	    // is the one GSD's `check auto-mode` reads.
   912	    //
   913	    // F-4 — no agent or launch-shape condition belongs in the predicate.
   914	    // `run_monitor` is the body of the hidden `__monitor` subcommand, which
   915	    // `monitor::spawn_monitor` re-execs ONLY on its `MonitorLaunch::PipeOwning`
   916	    // arm. Being inside this function already implies a stream launch (Claude
   917	    // and Antigravity today, round-3 D-10) — the old "Claude + stream launch"
   918	    // claim became false when the predicate widened — so re-checking
   919	    // `state.agent` or `state.legacy_claude_launch` here would be a second,
   920	    // driftable notion of the same fact. The consequence — a Legacy-arm or
   921	    // non-stream-agent launch never gets the flag — is accepted and is turned
   922	    // into a loud preflight refusal by plan `35.1-03`, not left silent.
   923	    //
   924	    // A state that will not load is NOT fatal here: warn, skip the guard, and
   925	    // let `advance` surface the real state error afterwards with its own
   926	    // context.
   927	    let _auto_chain_guard = match workflow::load_state(project_root, phase) {
   928	        Ok(state) => Some(AutoChainGuard::engage(
   929	            workdir,
   930	            auto_chain_flag_eligible(state.stage, state.mode),
  1110	    )
  1111	}
  1112	
  1113	/// Spawn the background monitor that owns the agent for `state.stage`. The
  1114	/// monitor calls `devflow advance` when the agent exits. An optional
  1115	/// `prompt_override` is used for Code loop-backs (fix prompts).
  1116	///
  1117	/// Resolves the prompt/adapter/roots/program, validates the agent binary,
  1118	/// then runs the readiness gate ([`run_preflight`]) before delegating to
  1119	/// [`launch_stage_inner`] for the actual spawn. Every EXISTING caller of
  1120	/// this function keeps getting the full path including preflight — the
  1121	/// ONLY caller of `launch_stage_inner` directly is `run_preflight`'s own
  1122	/// `Advance` arm (18f, D-18f), which is skipping a check it just
  1123	/// adjudicated for this one relaunch, not granting a standing bypass
  1124	/// (T-18-28: the skip must never leak beyond the single stage a human
  1125	/// approved).
  1126	pub(crate) fn launch_stage(
  1127	    state: &mut State,
  1128	    prompt_override: Option<String>,
  1129	    archived_stage: Option<Stage>,
  1130	) -> Result<(), CliError> {
  1131	    let driver = agents::driver_for(state.agent);
  1132	    let prompt = prompt_override.clone().unwrap_or_else(|| {
  1133	        driver.render_prompt(&prompt::StageIntent::for_stage_in_project(
  1134	            state.stage,
  1135	            state.phase,
  1136	            Some(&state.project_root),
  1137	        ))
  1138	    });
  1139	    let roots = state
  1140	        .worktree_path
  1141	        .as_deref()
  1142	        .map(|wt| worktree_writable_roots(&state.project_root, wt))
  1143	        .unwrap_or_default();
  1144	    let (program, _args) = driver.build_command(state.phase, &prompt, &roots);
  1145	    ensure_agent_binary(program)?;
  1146	
  1147	    // 17c (Task 1, D-13-D-16): a scoped readiness gate runs before any agent
  1148	    // time is spent — a failing check surfaces as a named preflight gate +
  1149	    // notify (never a hard exit, D-15), not here.
  1150	    //
  1151	    // CR-01 (17-08 gap closure): `run_preflight` returns `Ok(false)` when a
  1152	    // failing check was ALREADY resolved via a full retried launch (or an
  1153	    // abort) — this frame must not run any more launch steps in that case,
  1154	    // or the agent gets spawned a second time for the same stage.
  1155	    let project_root = state.project_root.clone();
  1156	    if !run_preflight(&project_root, state, driver.as_ref())? {
  1157	        return Ok(());
  1158	    }
  1159	
  1160	    launch_stage_inner(state, prompt_override, archived_stage)
  1161	}
  1162	
  1163	/// Route an `Ambiguous` outcome from the PRIMARY advance() monitor loop
  1164	/// (A2, 41-antigravity UAT): the agent's own final message self-reported
  1165	/// success, but the CLI's result envelope was torn down by a transport-level
  1166	/// cancellation (`context canceled` / `context deadline exceeded`). The stage
  1167	/// is RE-DRIVEN — the same stage is relaunched with the same prompt — rather
  1168	/// than gated (the agent already succeeded) or advanced (a torn envelope is
  1169	/// not proof of a clean finish).
  1170	///
   280	    /// relaunch all run through here, unchanged.
   281	    Legacy,
   282	}
   283	
   284	/// Spawn a background monitor that owns the agent for the given workflow state.
   285	///
   286	/// The monitor is a detached process that:
   287	/// 1. Launches the agent (`program` + `args`) with stdout captured to the
   288	///    phase stdout file, recording the agent PID to the agent-pid file
   289	/// 2. Waits for the agent to exit and records its exit code to the exit file
   290	/// 3. Runs `devflow advance --phase N` to advance the workflow through its
   291	///    remaining stages
   292	///
   293	/// `launch` selects the supervision shape — see [`MonitorLaunch`].
   294	///
   295	/// Returns the PID of the spawned monitor.
   296	pub fn spawn_monitor(
   297	    state: &State,
   298	    program: &str,
   299	    args: &[String],
   300	    envs: &[(String, String)],
   301	    launch: MonitorLaunch,
   302	) -> Result<u32, MonitorError> {
   303	    spawn_monitor_inner(state, program, args, envs, launch, true)
   304	}
   305	
   306	fn spawn_monitor_inner(
   307	    state: &State,
   308	    program: &str,
   309	    args: &[String],
   310	    envs: &[(String, String)],
   311	    launch: MonitorLaunch,
   312	    run_advance: bool,
   313	) -> Result<u32, MonitorError> {
   314	    let project_root = state
   315	        .project_root
   316	        .to_str()
   317	        .ok_or(MonitorError::NonUtf8Path)?;
   318	
   319	    let binary = std::env::current_exe()
   320	        .map_err(|_| MonitorError::NoBinaryPath)?
   321	        .to_str()
   322	        .ok_or(MonitorError::NonUtf8Path)?
   323	        .to_string();
   324	
   325	    info!(
   326	        "spawning monitor for phase {}: {program} {}",
   327	        state.phase,
   328	        args.join(" ")
   329	    );
   330	
   331	    let stdout_file = crate::agent_result::stdout_path(&state.project_root, state.phase);
   332	    let stderr_file = crate::agent_result::stderr_path(&state.project_root, state.phase);
   333	    let exit_file = crate::agent_result::exit_code_path(&state.project_root, state.phase);
   334	    let pid_file = crate::agent_result::agent_pid_path(&state.project_root, state.phase);
   335	
   336	    // Ensure the capture directory exists before the detached process runs.
   337	    if let Some(parent) = stdout_file.parent() {
   338	        crate::workflow::ensure_devflow_dir(parent)?;
   339	    }
   340	
   341	    let stdout_file = stdout_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
   342	    let stderr_file = stderr_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
   343	    let exit_file = exit_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
   344	    let pid_file = pid_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
   345	
   346	    // The agent runs in its worktree when worktree mode is active; otherwise it
   347	    // runs in the project root. Capture/state files and the `devflow check`
   348	    // calls below always use the main project root, regardless of cwd.
   349	    let workdir_path = state
   350	        .worktree_path
   351	        .as_deref()
   352	        .unwrap_or(&state.project_root);
   353	    let workdir = workdir_path.to_str().ok_or(MonitorError::NonUtf8Path)?;
   354	
   355	    if let MonitorLaunch::PipeOwning { prompt } = launch {
   356	        // `run_advance` is not consulted on this arm: the `__monitor`
   357	        // subcommand always advances after reaping, and `spawn_monitor` is the
   358	        // only caller of this function — it hardcodes `true`. Adding a
   359	        // `--no-advance` flag for a case nothing exercises would be an
   360	        // untested branch; add it when a caller actually needs it.
   361	        let _ = run_advance;
   362	
   363	        // The adapter's extra env rides down by INHERITANCE here (set via
   364	        // `.envs(...)` on the `__monitor` process below), and that is only
   365	        // sufficient because the adapters routed through this arm — Claude
   366	        // and Antigravity (round-3) — declare no extra env at all
   367	        // (`codex_disables_signing_via_env_others_do_not` asserts this).
   368	        // Widening this arm to an adapter that DOES set env requires
   369	        // threading it explicitly to `run_pipe_owning_monitor`: the inner
   370	        // `hermetic_command` scrubs `GIT_CONFIG_COUNT`, which neutralises any
   371	        // inherited `GIT_CONFIG_KEY_n` pair (Codex's unsigned-commit
   372	        // override is exactly that shape). Loud rather than silent, and in
   373	        // the CLI process where an operator can actually see it.
   374	        if !envs.is_empty() {
   375	            warn!(
   376	                "pipe-owning monitor: {} adapter env var(s) will not survive the \
   377	                 inner hermetic_command scrub — thread them explicitly before \
   378	                 routing an env-setting adapter through this arm",
   379	                envs.len()
   380	            );
   381	        }
   382	
   383	        // D-04: resolve and clamp the idle timeout HERE, in the parent, and
   384	        // hand the monitor the already-resolved integer.
   385	        //
   386	        // The placement is the whole point. `spawn_monitor` runs inside
   387	        // `devflow start`, attached to the operator's terminal; the monitor is
   388	        // a detached process whose stdio is all `Stdio::null()`, so a warning
   389	        // logged there scrolls into nothing. A silent clamp is the exact
   390	        // failure class this project keeps paying for, so the notice goes to
   391	        // BOTH `tracing::warn!` and stdout — the log for the record, stdout
   392	        // for the human who is watching right now.
   393	        let idle = idle_timeout_setting_for(state.agent);
   394	        // The notice names the variable the operator actually set (D-08): the
   395	        // literal is deliberately spelled here so `doc_check` keeps BOTH
   396	        // variables visible to the operator-doc parity gate.
   397	        let idle_env = match state.agent {
   398	            AgentKind::Antigravity => "DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS",
   399	            _ => IDLE_TIMEOUT_ENV,
   400	        };
   401	        if let Some(notice) = idle.notice_for(idle_env) {
   402	            warn!("{notice}");
   403	            println!("{notice}");
   404	        }
   405	
   406	        // The prompt travels as a FILE, not argv: argv has a hard length
   407	        // ceiling and DevFlow stage prompts routinely exceed what is safe to
   408	        // pass positionally.
   409	        let prompt_file = crate::agent_result::prompt_path(&state.project_root, state.phase);
   410	        std::fs::write(&prompt_file, &prompt)?;
   411	        let prompt_file = prompt_file.to_str().ok_or(MonitorError::NonUtf8Path)?;
   412	
   413	        // Re-exec THIS binary as its hidden `__monitor` subcommand. The
   414	        // monitor must outlive `devflow start`/`advance`, so it has to be a
   415	        // distinct OS process; re-exec needs no daemonization primitive beyond
   416	        // `spawn()`-without-`wait()`, which is exactly what the `sh` monitor
   417	        // below already relies on.
   418	        //
   419	        // Ordering is load-bearing for the same reason the Legacy arm's
   420	        // comment gives: `hermetic_command` does its `env_remove`s at
   421	        // construction and `.envs(...)` runs after, so deliberate
   422	        // configuration survives while inherited pollution does not.
   423	        let child = hermetic_command(&binary, workdir_path)
   424	            .arg("__monitor")
   425	            .arg("--project")
   426	            .arg(project_root)
   427	            .arg("--phase")
   428	            .arg(state.phase.to_string())
   429	            .arg("--workdir")
   430	            .arg(workdir)
   431	            .arg("--prompt-file")
   432	            .arg(prompt_file)
   433	            .arg("--idle-timeout-secs")
   434	            .arg(idle.timeout.as_secs().to_string())
   435	            .arg("--agent")
   436	            .arg(state.agent.to_string())
   437	            .arg("--")
   438	            .arg(program)
   439	            .args(args)
   440	            .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
   441	            .stdin(Stdio::null())
   442	            .stdout(Stdio::null())
   443	            .stderr(Stdio::null())
   444	            .spawn()?;
   445	
   446	        let pid = child.id();
   447	        info!("pipe-owning monitor spawned with pid {pid}");
   448	        return Ok(pid);
   449	    }
   450	
   451	    // Shell script that launches the agent in the background, captures its
   452	    // stdout and exit code, then advances the workflow. Because this process
   453	    // is the agent's parent, capture survives the CLI exiting.
   454	    //
   455	    // stderr is captured to a separate file so it cannot corrupt the (possibly
   456	    // JSON) stdout capture that DevFlow parses for DEVFLOW_RESULT. Inspect
   457	    // .devflow/phase-NN-stderr.log for agent error output on failures.
   458	    //
   459	    // `devflow advance --phase N` evaluates the agent result, moves the stage
   460	    // machine forward, and (for an agent stage) spawns the next monitor
   480	    } else {
   481	        String::new()
   482	    };
   483	    let script = format!(
   484	        "apid=''; cleanup() {{ [ -n \"$apid\" ] && kill \"$apid\" 2>/dev/null; exit 0; }}; \
   485	         trap cleanup TERM INT; \
   486	         cd {workdir} || exit 1; \
   487	         \"$@\" > {stdout_file} 2>{stderr_file} & \
   488	         apid=$!; echo $apid > {pid_file}; \
   489	         wait $apid; echo $? > {exit_file}{advance_tail}",
   490	        workdir = shell_escape(workdir),
   491	        stdout_file = shell_escape(stdout_file),
   492	        stderr_file = shell_escape(stderr_file),
   493	        exit_file = shell_escape(exit_file),
   494	        pid_file = shell_escape(pid_file),
   495	    );
   496	
   497	    // 27-REVIEW WR-03: built through `hermetic_command`, not a bare
   498	    // `Command::new("sh")`. This is the spawn that launches the coding agent
   499	    // itself, and the comment below is precisely the hazard: whatever
   500	    // environment this `sh` carries rides down into the agent and into every
   501	    // git command the agent runs. An inherited `GIT_DIR` here would silently
   502	    // retarget the phase's real commits at a repository the operator never
   503	    // named — the worst case this phase exists to prevent, on its
   504	    // highest-consequence call site.
   505	    //
   506	    // Ordering is load-bearing: `hermetic_command` does its `env_remove`s at
   507	    // construction, and `.envs(...)` below runs after, so an adapter that
   508	    // deliberately sets one of these variables still wins. Deliberate
   509	    // configuration survives; inherited pollution does not. That is what
   510	    // keeps Codex's unsigned-commit override (`GIT_CONFIG_*`) working.
   511	    let child = hermetic_command("sh", workdir_path)
   512	        .arg("-c")
   513	        .arg(&script)
   514	        .arg("sh")
   515	        .arg(program)
   516	        .args(args)
   517	        // Adapter-scoped env (e.g. Codex's unsigned-commit override) rides
   518	        // the whole monitor chain: sh → agent → its git children (13-06).
   519	        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
   520	        .stdin(Stdio::null())
   521	        .stdout(Stdio::null())
   522	        .stderr(Stdio::null())
   523	        .spawn()?;
   524	
   525	    let pid = child.id();
   526	    info!("monitor spawned with pid {pid}");
   527	    Ok(pid)
   528	}
   529	
   530	/// Constraint 4's close rule as a pure, line-fed state machine: stdin may be
   531	/// released only once a `DEVFLOW_RESULT` marker has appeared inside a
   532	/// TOP-LEVEL `result` event **and** the background-task list has drained.
   533	///
   534	/// An `AND` of two arms, neither sufficient alone:
   535	///
   536	/// - **Marker arm.** Satisfied only by
   537	///   [`crate::agent_result::event_is_top_level_result_marker`] — a composition
   538	///   of the existing `is_top_level` predicate and the existing marker parser,
   539	///   never a looser text search. The CLI echoes the operator's prompt back
   540	///   into the same stdout, and DevFlow's own stage prompts discuss
   541	///   `DEVFLOW_RESULT` markers at length, so marker text alone is not evidence
   542	///   (T-31-01; the same echo produced the checkpoint false positive 30-05
   543	///   fixed).
   544	/// - **Drain arm.** Satisfied when no `background_tasks_changed` event has
   545	///   ever announced anything (vacuous — the common single-plan case) or when
   850	/// - the **supervisor** (this function's own thread), which applies the close
   851	///   rule and reaps.
   852	///
   853	/// The write and the read MUST be on independent threads. Writing the prompt
   854	/// synchronously before reading stdout is the textbook two-pipe deadlock: it
   855	/// passes every short-prompt smoke test and hangs on exactly the context-heavy
   856	/// production stages that matter (the Linux pipe buffer is commonly 64KiB and
   857	/// a DevFlow stage prompt can exceed that in one write).
   858	#[allow(clippy::too_many_arguments)]
   859	pub fn run_pipe_owning_monitor(
   860	    project_root: &Path,
   861	    phase: PhaseId,
   862	    workdir: &Path,
   863	    prompt: &str,
   864	    idle_timeout: Duration,
   865	    program: &str,
   866	    args: &[String],
   867	    envs: &[(String, String)],
   868	    agent: AgentKind,
   869	) -> Result<i32, MonitorError> {
   870	    let stdout_file = crate::agent_result::stdout_path(project_root, phase);
   871	    let stderr_file = crate::agent_result::stderr_path(project_root, phase);
   872	    let exit_file = crate::agent_result::exit_code_path(project_root, phase);
   873	    let pid_file = crate::agent_result::agent_pid_path(project_root, phase);
   874	    if let Some(parent) = stdout_file.parent() {
   875	        crate::workflow::ensure_devflow_dir(parent)?;
   876	    }
   877	
   878	    // stderr goes to its own file so it cannot corrupt the JSONL stdout
   879	    // capture DevFlow parses — the same separation the Legacy script's
   880	    // `2>{stderr_file}` provides.
   881	    let stderr_handle = std::fs::File::create(&stderr_file)?;
   882	    // One handle, opened once, truncating at open and appending line by line.
   883	    // Truncate-at-open reproduces the Legacy arm's `>` redirection exactly, so
   884	    // a capture from a previous attempt can never be mixed into this one's
   885	    // (the launch path archives the prior capture first, but relying on that
   886	    // to make an append-mode open safe would be an unstated coupling).
   887	    let mut capture = std::fs::File::create(&stdout_file)?;
   888	
   889	    let mut child = hermetic_command(program, workdir)
   890	        .args(args)
   891	        .envs(envs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
   892	        .stdin(Stdio::piped())
   893	        .stdout(Stdio::piped())
   894	        .stderr(Stdio::from(stderr_handle))
   895	        // T-31-05: make the child its own process-group leader so a later
   896	        // group signal cannot reach this monitor's own ancestors. Verified
   897	        // source shows the pre-31 `spawn_monitor` had NO session or group
   898	        // configuration at all — detachment came only from the parent not
   899	        // waiting — so this closes a gap rather than preserving one.
   900	        // Full `setsid()` session detachment is deliberately NOT done: no
   901	        // forensics record cites a SIGHUP-related monitor loss, so there is
   902	        // no evidence it buys anything. `pre_exec` calling `libc::setsid()`
   903	        // is the one-line follow-on if such a loss ever surfaces.
   904	        .process_group(0)
   905	        .spawn()?;
   906	
   907	    // Recorded immediately, before any pipe work: `wait_for_agent_pid` polls
   908	    // for this and the rest of DevFlow's liveness reporting depends on it.
   909	    let child_pid = child.id();
   910	    std::fs::write(&pid_file, format!("{child_pid}\n"))?;
   911	
   912	    let mut child_stdin = child
   913	        .stdin
   914	        .take()
   915	        .ok_or(MonitorError::NoChildPipe("stdin"))?;

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "nl -ba crates/devflow-cli/src/commands.rs | sed -n '2160,2245p;2260,2335p'; nl -ba crates/devflow-cli/src/preflight.rs | sed -n '1120,1205p'" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 201ms:
  2160	            .arg("-c")
  2161	            .arg(cmd)
  2162	            .status()
  2163	            .map_err(|err| CliError::Message(format!("could not run `{cmd}`: {err}")))?;
  2164	        if status.success() {
  2165	            println!("  ✓ {label}");
  2166	        } else {
  2167	            println!("  ✗ {label}");
  2168	            failures.push(label);
  2169	        }
  2170	    }
  2171	    if failures.is_empty() {
  2172	        println!("\nall checks passed");
  2173	        Ok(())
  2174	    } else {
  2175	        Err(CliError::Message(format!(
  2176	            "quality checks failed: {}",
  2177	            failures.join(", ")
  2178	        )))
  2179	    }
  2180	}
  2181	
  2182	// ---------------------------------------------------------------------------
  2183	// doctor
  2184	// ---------------------------------------------------------------------------
  2185	
  2186	/// One tool/environment check from `doctor`'s pre-existing audit (git,
  2187	/// cargo, agent CLIs, `RUST_LOG`, ...). Module-level (WR-01, 18-fix) so
  2188	/// `checks_json_value` and `doctor_json_body` can compose it into
  2189	/// `doctor --json`'s single output document without living inside `doctor`
  2190	/// itself.
  2191	pub(crate) struct Check {
  2192	    pub(crate) name: String,
  2193	    pub(crate) status: String,
  2194	    pub(crate) version: Option<String>,
  2195	    pub(crate) install_hint: Option<String>,
  2196	}
  2197	
  2198	/// The environment checks `doctor` reports (phase 41 Task 7, F7): a named,
  2199	/// module-level seam so a unit test can assert the LIST without invoking the
  2200	/// whole doctor flow. `doctor()` calls this and renders the result. Presence
  2201	/// probes only — never a hard failure when a binary is absent (D-04).
  2202	fn doctor_checks() -> Vec<Check> {
  2203	    use std::process::Command;
  2204	
  2205	    fn cmd_check(name: &str, cmd: &str, version_arg: &str, install_hint: &str) -> Check {
  2206	        match Command::new(cmd).arg(version_arg).output() {
  2207	            Ok(out) if out.status.success() => {
  2208	                let version = String::from_utf8_lossy(&out.stdout)
  2209	                    .lines()
  2210	                    .next()
  2211	                    .unwrap_or("unknown")
  2212	                    .trim()
  2213	                    .to_string();
  2214	                Check {
  2215	                    name: name.into(),
  2216	                    status: "ok".into(),
  2217	                    version: Some(version),
  2218	                    install_hint: None,
  2219	                }
  2220	            }
  2221	            Ok(out) => {
  2222	                let detail = String::from_utf8_lossy(&out.stderr)
  2223	                    .lines()
  2224	                    .next()
  2225	                    .unwrap_or("unknown")
  2226	                    .trim()
  2227	                    .to_string();
  2228	                Check {
  2229	                    name: name.into(),
  2230	                    status: "warn".into(),
  2231	                    version: Some(detail),
  2232	                    install_hint: Some(format!(
  2233	                        "`{cmd} {version_arg}` exited non-zero — reinstall or check PATH"
  2234	                    )),
  2235	                }
  2236	            }
  2237	            Err(_) => Check {
  2238	                name: name.into(),
  2239	                status: "missing".into(),
  2240	                version: None,
  2241	                install_hint: Some(install_hint.into()),
  2242	            },
  2243	        }
  2244	    }
  2245	
  2260	            "warn",
  2261	            Some("empty (logging disabled)".into()),
  2262	            Some("Set RUST_LOG=info for better diagnostics".into()),
  2263	        ),
  2264	        Ok(val) => {
  2265	            let all_valid = val.split(',').all(|directive| {
  2266	                let directive = directive.trim();
  2267	                if let Some((_target, level)) = directive.split_once('=') {
  2268	                    matches!(level.trim(), "error" | "warn" | "info" | "debug" | "trace")
  2269	                } else {
  2270	                    matches!(directive, "error" | "warn" | "info" | "debug" | "trace")
  2271	                }
  2272	            });
  2273	            if all_valid {
  2274	                ("ok", Some(val), None)
  2275	            } else {
  2276	                (
  2277	                    "warn",
  2278	                    Some(val),
  2279	                    Some("RUST_LOG value may be invalid — expected error, warn, info, debug, or trace".into()),
  2280	                )
  2281	            }
  2282	        }
  2283	        Err(_) => (
  2284	            "missing",
  2285	            Some("not set — defaulting to info".into()),
  2286	            Some("Set RUST_LOG=info for better diagnostics".into()),
  2287	        ),
  2288	    };
  2289	
  2290	    vec![
  2291	        cmd_check(
  2292	            "git",
  2293	            "git",
  2294	            "--version",
  2295	            "Install from https://git-scm.com/downloads",
  2296	        ),
  2297	        bool_check("sh (POSIX shell)", cfg!(unix), "built-in", "Unsupported OS"),
  2298	        cmd_check(
  2299	            "cargo/rust",
  2300	            "cargo",
  2301	            "--version",
  2302	            "curl https://sh.rustup.rs -sSf | sh",
  2303	        ),
  2304	        cmd_check(
  2305	            "gh CLI",
  2306	            "gh",
  2307	            "--version",
  2308	            "brew install gh / apt install gh",
  2309	        ),
  2310	        cmd_check(
  2311	            "claude",
  2312	            "claude",
  2313	            "--version",
  2314	            "npm i -g @anthropic-ai/claude-code",
  2315	        ),
  2316	        cmd_check("codex", "codex", "--version", "npm i -g @openai/codex"),
  2317	        cmd_check(
  2318	            "opencode",
  2319	            "opencode",
  2320	            "--version",
  2321	            "cargo install opencode",
  2322	        ),
  2323	        cmd_check(
  2324	            "pi",
  2325	            "pi",
  2326	            "--version",
  2327	            "Install Pi (see https://github.com/earendil-works/pi-mono)",
  2328	        ),
  2329	        // Phase 41 Task 7 (D-04/F7): presence-only probe of the operator's
  2330	        // `agy` wrapper. `agy --version` reports the CLI version WITHOUT
  2331	        // invoking the model — the `-p --help` hazard (a Go-flag string flag
  2332	        // that swallows the next token) does not apply to `--version`.
  2333	        cmd_check(
  2334	            "antigravity",
  2335	            "agy",
  1120	            "GSD config can hold the chain flag",
  1121	            unattended_config_condition(launch_root),
  1122	        ),
  1123	        (
  1124	            "Code would launch on the pipe-owning arm",
  1125	            unattended_launch_shape_condition(state),
  1126	        ),
  1127	        (
  1128	            "no plan declares a human-only checkpoint",
  1129	            unattended_planned_checkpoint_condition(launch_root, state),
  1130	        ),
  1131	    ];
  1132	
  1133	    // UNCONDITIONAL, and above the mode branch below on purpose (D-08).
  1134	    let _ = writeln!(
  1135	        report,
  1136	        "unattended-launch prerequisites — phase {phase}, stage {stage}, mode {mode}:",
  1137	        phase = state.phase,
  1138	        stage = state.stage,
  1139	        mode = state.mode
  1140	    );
  1141	    for (name, condition) in &conditions {
  1142	        let _ = match condition.detail() {
  1143	            Some(detail) => writeln!(report, "  [{}] {name} — {detail}", condition.label()),
  1144	            None => writeln!(report, "  [{}] {name}", condition.label()),
  1145	        };
  1146	    }
  1147	
  1148	    // D-08: supervise reports and proceeds. The evaluation above already ran,
  1149	    // so the rehearsal is of the real check and not of a cheaper stand-in.
  1150	    if state.mode != Mode::Auto {
  1151	        return Ok(());
  1152	    }
  1153	
  1154	    // The LABEL is carried into the refusal string, not left in the report
  1155	    // alone. "does not hold" and "could not be determined" call for different
  1156	    // operator actions — fix the condition versus go and look at why it could
  1157	    // not be observed — and the gate context is the only place some operators
  1158	    // will ever read. Keeping the distinction internal would make the
  1159	    // three-state design unobservable from outside, which is indistinguishable
  1160	    // from not having it.
  1161	    let refusals: Vec<String> = conditions
  1162	        .iter()
  1163	        .filter(|(_, condition)| condition.refuses())
  1164	        .filter_map(|(name, condition)| {
  1165	            condition
  1166	                .detail()
  1167	                .map(|detail| format!("{} [{}] — {detail}", name, condition.label()))
  1168	        })
  1169	        .collect();
  1170	    if refusals.is_empty() {
  1171	        return Ok(());
  1172	    }
  1173	    Err(format!(
  1174	        "unattended launch refused (D-07) — {}",
  1175	        refusals.join("; ")
  1176	    ))
  1177	}
  1178	
  1179	/// The generic (universal) preflight checks (D-14) — the adapter-specific
  1180	/// hook is composed separately in [`run_preflight`].
  1181	///
  1182	/// CR-01 (`25-REVIEW.md`, 25-08): runs all three checks unconditionally and
  1183	/// aggregates every `Err` into one reason, rather than `?`-short-circuiting
  1184	/// on the first failure. `run_preflight`'s `GateAction::Advance` arm
  1185	/// relaunches via `launch_stage_inner` directly and never re-runs this
  1186	/// function — so under the old `?`-chain, a check that never ran once
  1187	/// (because an earlier check in the chain failed first) would never run at
  1188	/// all for that stage launch, and a human approving that earlier gate would
  1189	/// never have been shown the unrun check's reason. Aggregation closes that
  1190	/// hole for every check in this chain, not just the major-bump check that
  1191	/// surfaced it (`25-VERIFICATION.md`'s named fix, `25-REVIEW.md`'s option
  1192	/// (a); option (b), special-casing the `Advance` arm instead, was
  1193	/// deliberately not taken — it would not close the same hole for a future
  1194	/// check added to this chain).
  1195	///
  1196	/// Reasons are ordered by consequence, **major-bump FIRST**, then
  1197	/// interactivity, then gh-auth — load-bearing, not cosmetic:
  1198	/// `run_preflight` passes the joined string through [`truncate_reason`] (a
  1199	/// hard 300-character cap) before it reaches the gate context, and the
  1200	/// major-bump reason is both the longest of the three and the only one
  1201	/// whose loss would silently re-open the unattended-ship hole D-09 exists
  1202	/// to close.
  1203	///
  1204	/// The adapter-specific hook (composed by [`run_preflight`] via
  1205	/// `.and_then`) is deliberately NOT folded into this aggregation —

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "nl -ba crates/devflow-cli/src/commands.rs | sed -n '2320,2375p'; nl -ba crates/devflow-core/src/state.rs | sed -n '300,475p'; rg -n \"fn stream_launch_enabled|STREAM_JSON_STAGES|legacy_claude_launch\" crates/devflow-cli/src/preflight.rs crates/devflow-cli/src/pipeline_launch.rs" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 172ms:
  2320	            "--version",
  2321	            "cargo install opencode",
  2322	        ),
  2323	        cmd_check(
  2324	            "pi",
  2325	            "pi",
  2326	            "--version",
  2327	            "Install Pi (see https://github.com/earendil-works/pi-mono)",
  2328	        ),
  2329	        // Phase 41 Task 7 (D-04/F7): presence-only probe of the operator's
  2330	        // `agy` wrapper. `agy --version` reports the CLI version WITHOUT
  2331	        // invoking the model — the `-p --help` hazard (a Go-flag string flag
  2332	        // that swallows the next token) does not apply to `--version`.
  2333	        cmd_check(
  2334	            "antigravity",
  2335	            "agy",
  2336	            "--version",
  2337	            "Install the Antigravity CLI so `agy` is on PATH (wrapper injects --dangerously-skip-permissions)",
  2338	        ),
  2339	        // Phase 42 Task 3 (HRMS-01, D-06): presence-only probe of the `hermes` binary.
  2340	        cmd_check(
  2341	            "hermes",
  2342	            "hermes",
  2343	            "--version",
  2344	            "Install the Hermes Agent CLI so `hermes` is on PATH",
  2345	        ),
  2346	        pi_subagent_dispatch_check(),
  2347	        Check {
  2348	            name: format!("devflow v{devflow_version}"),
  2349	            status: "ok".into(),
  2350	            version: Some(devflow_version.into()),
  2351	            install_hint: None,
  2352	        },
  2353	        Check {
  2354	            name: "RUST_LOG".into(),
  2355	            status: rust_log_status.into(),
  2356	            version: rust_log_version,
  2357	            install_hint: rust_log_hint,
  2358	        },
  2359	    ]
  2360	}
  2361	
  2362	/// Audit the environment and report what's installed, missing, or broken.
  2363	pub(crate) fn doctor(project_root: &Path, json: bool) -> Result<(), CliError> {
  2364	    let checks = doctor_checks();
  2365	
  2366	    let facts = collect_phase_facts(project_root);
  2367	    let doc_findings = collect_planning_doc_findings(project_root);
  2368	    // 999.44/DEN-68: a registry-independent, read-only /proc census — the
  2369	    // only I/O this adds is `agent::discover_stray_devflow_processes`'s
  2370	    // scan, which never signals anything (T-25-62, doctor's read-only
  2371	    // contract).
  2372	    let stray_findings = collect_stray_process_findings(project_root);
  2373	
  2374	    if json {
  2375	        // WR-01 (18-fix): a single top-level JSON document —
   300	    /// How many times the current stage's agent has been relaunched via a
   301	    /// checkpoint auto-decide resume (D-04, 28-03). Bounds a stuck
   302	    /// checkpoint loop against `mode::MAX_CHECKPOINT_RESUMES` (added in plan
   303	    /// 28-03) the same way [`Self::infra_failures`] bounds an infra-fault
   304	    /// loop against `mode::MAX_INFRA_FAILURES`. Reset to 0 by every ordinary fresh stage
   305	    /// launch, so the ceiling bounds one stage's resume budget, not a
   306	    /// phase's lifetime (the same distinction `MAX_INFRA_FAILURES`' doc
   307	    /// comment draws for `infra_failures`). Any increment must use
   308	    /// `saturating_add` so a stuck loop cannot overflow `u32`. A
   309	    /// serde-absent value (state written by a binary predating this field)
   310	    /// defaults to 0.
   311	    #[serde(default)]
   312	    pub checkpoint_resumes: u32,
   313	    /// The stage `devflow start --until <stage>` requests as the last stage
   314	    /// to run before halting (20c). `None` means no stop point was
   315	    /// requested (the pipeline runs to Ship), OR the state was written by a
   316	    /// binary predating this field — both cases behave identically (no
   317	    /// interception in `transition()`).
   318	    #[serde(default)]
   319	    pub stop_until: Option<Stage>,
   320	    /// Set by `transition()` when `stop_until` names the stage just
   321	    /// completed — a terminal-but-not-failed halt short of Ship (20c).
   322	    /// `false` for a normal in-flight or completed-to-Ship phase, and for
   323	    /// any state written by a binary predating this field.
   324	    #[serde(default)]
   325	    pub stopped: bool,
   326	    /// Human-readable reason recorded alongside `stopped` (20c). `None`
   327	    /// when `stopped` is `false`, or when the state predates this field.
   328	    #[serde(default)]
   329	    pub stop_reason: Option<String>,
   330	    /// Pre-authorization for the Ship gate (D-04/D-05/D-06, 23-09),
   331	    /// set only from the `--yes-ship` CLI flag typed on `devflow start`.
   332	    ///
   333	    /// Persisted rather than passed through the call stack: the Ship gate
   334	    /// fires inside a detached monitor's `advance` process, minutes to
   335	    /// hours after the launching `devflow start` process has already
   336	    /// exited, so a CLI-scoped value would be gone by the time it matters —
   337	    /// only a value written to `state.json` at start time survives to be
   338	    /// read back by that later, separate process. `false` for any state
   339	    /// written by a binary predating this field.
   340	    #[serde(default)]
   341	    pub yes_ship: bool,
   342	    /// What this run's delivery canary established (D-13/D-15, 31-03),
   343	    /// recorded by the first stage launch that routes through the Claude
   344	    /// `stream-json` transport. `None` means EITHER "no canary has run for
   345	    /// this run yet" OR "the state was written by a binary predating this
   346	    /// field" — both cases behave identically: the canary runs.
   347	    ///
   348	    /// Persisted rather than held in memory for the same reason
   349	    /// [`Self::yes_ship`] is: each stage launch happens in a SEPARATE
   350	    /// `devflow` process (the monitor's own `advance` tail), so an
   351	    /// in-process flag would reset to "not yet run" at every stage
   352	    /// transition and re-spend a real throwaway agent invocation each time —
   353	    /// which is exactly the symptom 31-RESEARCH Pitfall 5 names for a canary
   354	    /// that landed in the per-stage `preflight` hook.
   355	    ///
   356	    /// A recorded `Absent`/`Unverified` keeps refusing on every later launch
   357	    /// in the run; it is not consumed by the first refusal.
   358	    #[serde(default)]
   359	    pub canary: Option<crate::canary::CanaryOutcome>,
   360	    /// D-11's opt-out: force the pre-31 single-document Claude launch
   361	    /// (positional prompt, `--output-format json`, the `sh` monitor) for this
   362	    /// run, off by default.
   363	    ///
   364	    /// `false` means EITHER "the operator did not ask for the legacy path" OR
   365	    /// "the state was written by a binary predating this field" — both cases
   366	    /// behave identically: the D-09/D-10 rollout decides the transport, which
   367	    /// is the pre-existing behaviour.
   368	    ///
   369	    /// Persisted rather than passed through the call stack for the reason
   370	    /// [`Self::yes_ship`] gives: each stage launch happens in a SEPARATE
   371	    /// `devflow` process (the detached monitor's own `advance` tail), so a
   372	    /// CLI-scoped value would be gone by the time the second stage launches
   373	    /// and the run would silently revert to the stream transport mid-flight.
   374	    ///
   375	    /// Only ever OR-ed, never cleared, once set — see
   376	    /// `pipeline_launch::apply_legacy_launch_opt_out`. Clearing it on a plain
   377	    /// `devflow resume` would be the same silent-drop class as `stop_until`'s
   378	    /// old unconditional clear (999.60). To turn it back off, edit
   379	    /// `.devflow/state-NN.json` or start a new run.
   380	    #[serde(default)]
   381	    pub legacy_claude_launch: bool,
   382	}
   383	
   384	/// Supported coding agents.
   385	#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
   386	#[serde(rename_all = "lowercase")]
   387	pub enum AgentKind {
   388	    /// Anthropic Claude Code CLI.
   389	    Claude,
   390	    /// OpenAI Codex CLI.
   391	    Codex,
   392	    /// OpenCode CLI.
   393	    OpenCode,
   394	    /// Pi coding-agent harness.
   395	    Pi,
   396	    /// Antigravity CLI (`agy`), stream-json transport (phase 41).
   397	    Antigravity,
   398	    /// Hermes Agent CLI (`hermes`), headless oneshot (phase 42).
   399	    Hermes,
   400	}
   401	
   402	impl fmt::Display for AgentKind {
   403	    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
   404	        let name = match self {
   405	            AgentKind::Claude => "claude",
   406	            AgentKind::Codex => "codex",
   407	            AgentKind::OpenCode => "opencode",
   408	            AgentKind::Pi => "pi",
   409	            AgentKind::Antigravity => "antigravity",
   410	            AgentKind::Hermes => "hermes",
   411	        };
   412	        f.write_str(name)
   413	    }
   414	}
   415	
   416	impl FromStr for AgentKind {
   417	    type Err = AgentParseError;
   418	
   419	    fn from_str(value: &str) -> Result<Self, Self::Err> {
   420	        match value.to_ascii_lowercase().as_str() {
   421	            "claude" => Ok(AgentKind::Claude),
   422	            "codex" => Ok(AgentKind::Codex),
   423	            "opencode" | "open-code" => Ok(AgentKind::OpenCode),
   424	            "pi" => Ok(AgentKind::Pi),
   425	            "antigravity" => Ok(AgentKind::Antigravity),
   426	            "hermes" => Ok(AgentKind::Hermes),
   427	            other => Err(AgentParseError(other.to_string())),
   428	        }
   429	    }
   430	}
   431	
   432	/// Error returned when parsing an unsupported agent name.
   433	#[derive(Debug, Clone, thiserror::Error)]
   434	#[error("unsupported agent `{0}`; expected claude, codex, opencode, pi, antigravity, or hermes")]
   435	pub struct AgentParseError(String);
   436	
   437	impl State {
   438	    /// Create a new state for starting a phase at the [`Stage::Define`] stage.
   439	    pub fn new(phase: PhaseId, agent: AgentKind, mode: Mode, project_root: PathBuf) -> Self {
   440	        State {
   441	            stage: Stage::Define,
   442	            phase,
   443	            agent,
   444	            mode,
   445	            gate_pending: false,
   446	            consecutive_failures: 0,
   447	            infra_failures: 0,
   448	            preflight_retries: 0,
   449	            last_validate_failure_commit_count: None,
   450	            phase_validate_failures: 0,
   451	            last_verification_fingerprint: None,
   452	            verification_baseline_captured: false,
   453	            last_verification_mtime_nanos: None,
   454	            verification_run_nonce: None,
   455	            started_at: timestamp_now(),
   456	            project_root,
   457	            worktree_path: None,
   458	            monitor_pid: None,
   459	            session_id: None,
   460	            checkpoint_resumes: 0,
   461	            stop_until: None,
   462	            stopped: false,
   463	            stop_reason: None,
   464	            yes_ship: false,
   465	            canary: None,
   466	            legacy_claude_launch: false,
   467	        }
   468	    }
   469	}
   470	
   471	fn timestamp_now() -> String {
   472	    match SystemTime::now().duration_since(UNIX_EPOCH) {
   473	        Ok(duration) => format!("{}", duration.as_secs()),
   474	        Err(_) => String::from("0"),
   475	    }
crates/devflow-cli/src/pipeline_launch.rs:117:    let stream_launch = stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
crates/devflow-cli/src/pipeline_launch.rs:222:    /// The persisted `state.legacy_claude_launch`, written at `start`/`resume`
crates/devflow-cli/src/pipeline_launch.rs:232:            LegacyLaunchSource::PersistedState => "state:legacy_claude_launch",
crates/devflow-cli/src/pipeline_launch.rs:331:/// Combine D-11's two authorization sources into `state.legacy_claude_launch`.
crates/devflow-cli/src/pipeline_launch.rs:335:/// unconditional `state.legacy_claude_launch = flag || env` would silently flip
crates/devflow-cli/src/pipeline_launch.rs:346:    state.legacy_claude_launch = state.legacy_claude_launch || flag || env;
crates/devflow-cli/src/pipeline_launch.rs:707:const STREAM_JSON_STAGES: &[Stage] = &[
crates/devflow-cli/src/pipeline_launch.rs:724:/// evaluates purely on `STREAM_JSON_STAGES` membership.
crates/devflow-cli/src/pipeline_launch.rs:734:/// **What widens [`STREAM_JSON_STAGES`]:** a passing acceptance run (D-16/D-18
crates/devflow-cli/src/pipeline_launch.rs:753:pub(crate) fn stream_launch_enabled(agent: AgentKind, stage: Stage, legacy_opt_out: bool) -> bool {
crates/devflow-cli/src/pipeline_launch.rs:755:        && STREAM_JSON_STAGES.contains(&stage)
crates/devflow-cli/src/pipeline_launch.rs:762:/// **[`STREAM_JSON_STAGES`] above is the shape precedent and an IMPERFECT
crates/devflow-cli/src/pipeline_launch.rs:919:    // `state.agent` or `state.legacy_claude_launch` here would be a second,
crates/devflow-cli/src/pipeline_launch.rs:1240:    legacy_claude_launch: bool,
crates/devflow-cli/src/pipeline_launch.rs:1259:    if apply_legacy_launch_opt_out(&mut state, legacy_claude_launch) {
crates/devflow-cli/src/pipeline_launch.rs:1673:    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
crates/devflow-cli/src/pipeline_launch.rs:1676:    /// its stage being ABSENT from `STREAM_JSON_STAGES` — an incidental
crates/devflow-cli/src/pipeline_launch.rs:1708:        state.legacy_claude_launch = true;
crates/devflow-cli/src/pipeline_launch.rs:1762:    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
crates/devflow-cli/src/pipeline_launch.rs:1784:        state.legacy_claude_launch = true;
crates/devflow-cli/src/pipeline_launch.rs:1855:    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
crates/devflow-cli/src/pipeline_launch.rs:1888:        state.legacy_claude_launch = true;
crates/devflow-cli/src/pipeline_launch.rs:1942:    /// **Premise moved off `STREAM_JSON_STAGES` membership deliberately
crates/devflow-cli/src/pipeline_launch.rs:1969:        state.legacy_claude_launch = true;
crates/devflow-cli/src/pipeline_launch.rs:2527:    /// `Stage::Plan`'s absence from [`STREAM_JSON_STAGES`] as its premise. That
crates/devflow-cli/src/pipeline_launch.rs:2537:    /// fully-widened `STREAM_JSON_STAGES` and stayed green, while the
crates/devflow-cli/src/pipeline_launch.rs:2552:        state.legacy_claude_launch = true;
crates/devflow-cli/src/pipeline_launch.rs:2557:            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
crates/devflow-cli/src/pipeline_launch.rs:2617:        state.legacy_claude_launch = false;
crates/devflow-cli/src/pipeline_launch.rs:2620:            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
crates/devflow-cli/src/pipeline_launch.rs:3364:        state.legacy_claude_launch = opt_out;
crates/devflow-cli/src/pipeline_launch.rs:3381:            "Stage::Code must be in STREAM_JSON_STAGES for this test to mean anything"
crates/devflow-cli/src/pipeline_launch.rs:3387:            state.legacy_claude_launch
crates/devflow-cli/src/pipeline_launch.rs:3460:            !state.legacy_claude_launch,
crates/devflow-cli/src/pipeline_launch.rs:3466:            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
crates/devflow-cli/src/pipeline_launch.rs:3506:            Some("state:legacy_claude_launch"),
crates/devflow-cli/src/pipeline_launch.rs:3551:        state.legacy_claude_launch = true;
crates/devflow-cli/src/pipeline_launch.rs:3556:            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
crates/devflow-cli/src/pipeline_launch.rs:3633:            stream_launch_enabled(state.agent, state.stage, state.legacy_claude_launch);
crates/devflow-cli/src/pipeline_launch.rs:3707:            state.legacy_claude_launch,
crates/devflow-cli/src/pipeline_launch.rs:3715:        assert!(!never_opted_out.legacy_claude_launch);
crates/devflow-cli/src/preflight.rs:982:    if stream_launch_enabled(state.agent, Stage::Code, state.legacy_claude_launch)
crates/devflow-cli/src/preflight.rs:988:    if state.legacy_claude_launch {
crates/devflow-cli/src/preflight.rs:998:        // Defensive: reachable only if `STREAM_JSON_STAGES` ever narrows to
crates/devflow-cli/src/preflight.rs:1978:        // Premise moved off `STREAM_JSON_STAGES` membership deliberately
crates/devflow-cli/src/preflight.rs:1987:        state.legacy_claude_launch = true;
crates/devflow-cli/src/preflight.rs:2072:        // Premise moved off `STREAM_JSON_STAGES` membership deliberately
crates/devflow-cli/src/preflight.rs:2077:        state.legacy_claude_launch = true;
crates/devflow-cli/src/preflight.rs:3339:        state.legacy_claude_launch = true;
crates/devflow-cli/src/preflight.rs:3361:        state.legacy_claude_launch = true; // one condition failing, two holding
crates/devflow-cli/src/preflight.rs:3462:        state.legacy_claude_launch = true;
crates/devflow-cli/src/preflight.rs:3470:        state.legacy_claude_launch = false;
crates/devflow-cli/src/preflight.rs:3553:        state.legacy_claude_launch = true;
crates/devflow-cli/src/preflight.rs:3697:        state.legacy_claude_launch = true;

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "nl -ba crates/devflow-cli/src/pipeline_launch.rs | sed -n '700,765p'; rg -n \"Hermes|hermes\" crates/devflow-cli/src crates/devflow-core/src | cat" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 30ms:
   700	/// legacy path and only then refusing. This is a real change to unattended
   701	/// behaviour, accepted rather than mitigated: D-15 rejected both alternatives
   702	/// (warn-and-proceed fails unattended; falling back to the legacy path is a
   703	/// silent capability downgrade). On the capture run the canary returned
   704	/// `Confirmed` at Define, so the relocated refusal did not fire — the
   705	/// relocation is recorded here on the strength of the code path, not on the
   706	/// strength of having watched it refuse.
   707	const STREAM_JSON_STAGES: &[Stage] = &[
   708	    Stage::Define,
   709	    Stage::Plan,
   710	    Stage::Code,
   711	    Stage::Validate,
   712	    Stage::Ship,
   713	];
   714	
   715	/// Whether this launch should use the `stream-json` transport and the
   716	/// pipe-owning monitor.
   717	///
   718	/// **Agent coverage (round-3 D-10):** Claude and Antigravity. The
   719	/// `legacy_opt_out` term applies ONLY to Claude — `DEVFLOW_CLAUDE_LEGACY_LAUNCH`
   720	/// is an escape hatch for Claude's pre-31 single-document launch, and
   721	/// Antigravity has no single-document format, so the variable must never route
   722	/// it to `MonitorLaunch::Legacy` (stdin would be `/dev/null` and the child
   723	/// would silently fail; antigravity reviewer notice (b)). Antigravity
   724	/// evaluates purely on `STREAM_JSON_STAGES` membership.
   725	///
   726	/// **This is a SEQUENCING choice, not a behaviour prediction.** Constraint 1
   727	/// forbids deciding at launch time which stages will background work; it
   728	/// permits rolling a change out one stage at a time. The reason for
   729	/// sequencing at all is evidentiary (D-09): every gate fixture today is
   730	/// labelled SYNTHETIC in-source and no archived capture contains a prompt
   731	/// echo, so the stream parser's production correctness is currently
   732	/// *reasoned, not witnessed*.
   733	///
   734	/// **What widens [`STREAM_JSON_STAGES`]:** a passing acceptance run (D-16/D-18
   735	/// — a two-plan wave where both plans produce a `SUMMARY.md` and merge)
   736	/// producing the first real production `stream-json` capture to verify the
   737	/// parser against. Not a green unit suite, and not "the stage reported
   738	/// Success" — the completion oracle already scored the orphaned Phase 29 stage
   739	/// as Success.
   740	///
   741	/// **`legacy_opt_out` is D-11's escape hatch (31-04)**, and it is folded in
   742	/// HERE rather than checked separately at each use so that ONE predicate still
   743	/// governs the launch shape, the D-15 canary gate that protects it, and the
   744	/// loud notice. Two separate notions of "is this the stream path?" would be
   745	/// free to drift, and the drift would show up as a guard firing on a launch it
   746	/// does not protect — or, worse, not firing on one it does.
   747	///
   748	/// Note what the opt-out does NOT reach: `relaunch_checkpoint_session`
   749	/// hardcodes `MonitorLaunch::Legacy` and calls `spawn_agent_and_record`
   750	/// directly, so it never consults this predicate at all. That is a
   751	/// pre-existing, deliberate legacy route (see `MonitorLaunch::Legacy`'s own
   752	/// doc), recorded rather than silently covered.
   753	pub(crate) fn stream_launch_enabled(agent: AgentKind, stage: Stage, legacy_opt_out: bool) -> bool {
   754	    matches!(agent, AgentKind::Claude | AgentKind::Antigravity)
   755	        && STREAM_JSON_STAGES.contains(&stage)
   756	        && !(agent == AgentKind::Claude && legacy_opt_out)
   757	}
   758	
   759	/// The stages whose launch may set GSD's `workflow._auto_chain_active` flag
   760	/// (D-05, `35.1-CONTEXT.md`). One element, deliberately.
   761	///
   762	/// **[`STREAM_JSON_STAGES`] above is the shape precedent and an IMPERFECT
   763	/// analogy.** That constant lists all five stages because its effect is purely
   764	/// a DevFlow-internal transport choice with no upstream consequence: whichever
   765	/// stage it names, the only thing that changes is how DevFlow talks to the
crates/devflow-core/src/version.rs:775:///   documented out-of-scope limitation (review: Antigravity/Hermes MEDIUM)
crates/devflow-core/src/state.rs:398:    /// Hermes Agent CLI (`hermes`), headless oneshot (phase 42).
crates/devflow-core/src/state.rs:399:    Hermes,
crates/devflow-core/src/state.rs:410:            AgentKind::Hermes => "hermes",
crates/devflow-core/src/state.rs:426:            "hermes" => Ok(AgentKind::Hermes),
crates/devflow-core/src/state.rs:434:#[error("unsupported agent `{0}`; expected claude, codex, opencode, pi, antigravity, or hermes")]
crates/devflow-core/src/state.rs:1129:    // Phase 42 Task 2: AgentKind::Hermes (HRMS-01, D-05, D-06).
crates/devflow-core/src/state.rs:1133:    fn agent_kind_hermes_from_str_is_case_insensitive() {
crates/devflow-core/src/state.rs:1134:        assert_eq!("hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
crates/devflow-core/src/state.rs:1135:        assert_eq!("HERMES".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
crates/devflow-core/src/state.rs:1136:        assert_eq!("Hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
crates/devflow-core/src/state.rs:1140:    fn agent_kind_hermes_display_is_lowercase() {
crates/devflow-core/src/state.rs:1141:        assert_eq!(AgentKind::Hermes.to_string(), "hermes");
crates/devflow-core/src/state.rs:1145:    fn agent_kind_hermes_serde_round_trips_lowercase() {
crates/devflow-core/src/state.rs:1146:        let wire = serde_json::to_string(&AgentKind::Hermes).unwrap();
crates/devflow-core/src/state.rs:1147:        assert_eq!(wire, "\"hermes\"");
crates/devflow-core/src/state.rs:1149:        assert_eq!(back, AgentKind::Hermes);
crates/devflow-core/src/state.rs:1153:    fn agent_kind_hermes_error_message_lists_it() {
crates/devflow-core/src/state.rs:1156:            err.to_string().contains("hermes"),
crates/devflow-core/src/state.rs:1162:    fn agent_kind_hermes_driver_for_resolves() {
crates/devflow-core/src/state.rs:1164:            crate::agents::driver_for(AgentKind::Hermes).name(),
crates/devflow-core/src/state.rs:1165:            "Hermes"
crates/devflow-core/src/stage.rs:7://! - **Validate / Ship** are gate stages: they may fire a gate to Hermes (the
crates/devflow-core/src/stage.rs:41:    /// Whether this stage fires a gate to Hermes (Validate and Ship).
crates/devflow-cli/src/pipeline_outcomes.rs:117:    if instructions.hermes_cron.schedule.is_empty() {
crates/devflow-cli/src/pipeline_outcomes.rs:4594:                .hermes_cron
crates/devflow-cli/src/pipeline_outcomes.rs:4694:        assert!(instructions.hermes_cron.schedule.is_empty());
crates/devflow-core/src/ship.rs:3://! Holds the Hermes cron-instructions manifest (used to resume a rate-limited
crates/devflow-core/src/ship.rs:11:/// Manifest consumed by Hermes to resume a rate-limited DevFlow run later.
crates/devflow-core/src/ship.rs:24:    /// Hermes cron job definition derived from the retry timestamp.
crates/devflow-core/src/ship.rs:25:    pub hermes_cron: HermesCronJob,
crates/devflow-core/src/ship.rs:37:/// Hermes one-shot cron job payload.
crates/devflow-core/src/ship.rs:39:pub struct HermesCronJob {
crates/devflow-core/src/ship.rs:46:    /// Whether Hermes should remove the job after it runs.
crates/devflow-core/src/ship.rs:80:/// Persist Hermes cron instructions for the phase recorded inside them.
crates/devflow-core/src/ship.rs:93:/// Load a phase's Hermes cron instructions, or [`ShipError::Missing`] if
crates/devflow-core/src/ship.rs:156:/// Build a Hermes cron-instructions manifest for resuming the PRIMARY
crates/devflow-core/src/ship.rs:181:        hermes_cron: HermesCronJob {
crates/devflow-core/src/ship.rs:627:                .hermes_cron
crates/devflow-core/src/ship.rs:631:        assert!(!record.hermes_cron.command.contains("sequentagent"));
crates/devflow-core/src/ship.rs:632:        assert!(!record.hermes_cron.command.contains(" start"));
crates/devflow-core/src/ship.rs:633:        assert!(record.hermes_cron.once);
crates/devflow-core/src/ship.rs:641:        assert_ne!(record.hermes_cron.schedule, "* * * * *");
crates/devflow-core/src/ship.rs:642:        assert!(record.hermes_cron.schedule.is_empty());
crates/devflow-core/src/mode.rs:9://! - **Supervise:** Same pipeline, but Validate always fires a gate to Hermes →
crates/devflow-core/src/mode.rs:216:    /// Fire a Validate gate to Hermes → Human before Ship.
crates/devflow-core/src/gates.rs:1://! Gate file protocol — the handoff between DevFlow and a human (via Hermes).
crates/devflow-core/src/gates.rs:4://! `.devflow/gates/` and waits for a human (or the Hermes cron poller) to drop
crates/devflow-core/src/gates.rs:44:    /// Who responded (human name, or "hermes").
crates/devflow-core/src/events.rs:11://! Hermes plugin, web) can observe a running loop by tailing one file instead
crates/devflow-core/src/agents/hermes.rs:1://! Hermes coding-agent adapter (Phase 42).
crates/devflow-core/src/agents/hermes.rs:3://! Launches `hermes -z "<prompt>" --yolo --accept-hooks` in headless-safe oneshot mode.
crates/devflow-core/src/agents/hermes.rs:8://! Subagent dispatch capability is dynamically probed via `hermes tools list` checking for
crates/devflow-core/src/agents/hermes.rs:15:/// The modular driver for Hermes (Phase 42): headless `-z` oneshot launch,
crates/devflow-core/src/agents/hermes.rs:18:pub struct HermesDriver;
crates/devflow-core/src/agents/hermes.rs:20:impl AgentDriver for HermesDriver {
crates/devflow-core/src/agents/hermes.rs:22:        "Hermes"
crates/devflow-core/src/agents/hermes.rs:31:            subagent_dispatch: hermes_subagent_dispatch_available(),
crates/devflow-core/src/agents/hermes.rs:42:            "hermes",
crates/devflow-core/src/agents/hermes.rs:57:        // Presence-only probe of the `hermes` binary.
crates/devflow-core/src/agents/hermes.rs:58:        let output = std::process::Command::new("hermes")
crates/devflow-core/src/agents/hermes.rs:61:            .map_err(|e| format!("could not run `hermes --version`: {e}"))?;
crates/devflow-core/src/agents/hermes.rs:66:            Err(format!("`hermes --version` failed: {}", detail.trim()))
crates/devflow-core/src/agents/hermes.rs:71:/// Dynamically probe whether Hermes has the `delegation` toolset enabled.
crates/devflow-core/src/agents/hermes.rs:73:/// Runs `hermes tools list` and checks for both `enabled` and `delegation` in the output.
crates/devflow-core/src/agents/hermes.rs:74:pub fn hermes_subagent_dispatch_available() -> bool {
crates/devflow-core/src/agents/hermes.rs:75:    hermes_subagent_dispatch_available_with(|| {
crates/devflow-core/src/agents/hermes.rs:76:        std::process::Command::new("hermes")
crates/devflow-core/src/agents/hermes.rs:83:pub fn hermes_subagent_dispatch_available_with(
crates/devflow-core/src/agents/hermes.rs:93:    parse_hermes_tools_list_for_delegation(&stdout)
crates/devflow-core/src/agents/hermes.rs:96:/// Parse `hermes tools list` stdout to check if delegation toolset is enabled.
crates/devflow-core/src/agents/hermes.rs:97:pub fn parse_hermes_tools_list_for_delegation(stdout: &str) -> bool {
crates/devflow-core/src/agents/hermes.rs:115:    fn hermes_driver_name() {
crates/devflow-core/src/agents/hermes.rs:116:        let driver = HermesDriver;
crates/devflow-core/src/agents/hermes.rs:117:        assert_eq!(driver.name(), "Hermes");
crates/devflow-core/src/agents/hermes.rs:121:    fn hermes_driver_build_command() {
crates/devflow-core/src/agents/hermes.rs:122:        let driver = HermesDriver;
crates/devflow-core/src/agents/hermes.rs:124:        assert_eq!(prog, "hermes");
crates/devflow-core/src/agents/hermes.rs:137:    fn hermes_driver_environment() {
crates/devflow-core/src/agents/hermes.rs:138:        let driver = HermesDriver;
crates/devflow-core/src/agents/hermes.rs:144:    fn hermes_driver_render_prompt() {
crates/devflow-core/src/agents/hermes.rs:145:        let driver = HermesDriver;
crates/devflow-core/src/agents/hermes.rs:153:    fn parse_hermes_tools_list_delegation_enabled() {
crates/devflow-core/src/agents/hermes.rs:160:        assert!(parse_hermes_tools_list_for_delegation(sample));
crates/devflow-core/src/agents/hermes.rs:164:    fn parse_hermes_tools_list_delegation_disabled() {
crates/devflow-core/src/agents/hermes.rs:170:        assert!(!parse_hermes_tools_list_for_delegation(sample));
crates/devflow-core/src/agents/hermes.rs:174:    fn parse_hermes_tools_list_missing_delegation() {
crates/devflow-core/src/agents/hermes.rs:179:        assert!(!parse_hermes_tools_list_for_delegation(sample));
crates/devflow-core/src/agents/hermes.rs:183:    fn hermes_subagent_dispatch_with_mock() {
crates/devflow-core/src/agents/hermes.rs:191:        assert!(hermes_subagent_dispatch_available_with(success_output));
crates/devflow-core/src/agents/hermes.rs:200:        assert!(!hermes_subagent_dispatch_available_with(failure_output));
crates/devflow-core/src/agents/hermes.rs:203:        assert!(!hermes_subagent_dispatch_available_with(io_error));
crates/devflow-core/src/agents/mod.rs:144:/// A future driver (Antigravity, Hermes) plugs in by passing these — the
crates/devflow-core/src/agents/mod.rs:181:        AgentKind::Hermes => Box::new(HermesDriver),
crates/devflow-core/src/agents/mod.rs:188:pub mod hermes;
crates/devflow-core/src/agents/mod.rs:195:pub use hermes::HermesDriver;
crates/devflow-core/src/agents/mod.rs:212:        assert_eq!(driver_for(AgentKind::Hermes).name(), "Hermes");
crates/devflow-core/src/agents/mod.rs:292:            Box::new(HermesDriver),
crates/devflow-core/src/agents/mod.rs:353:    /// Phase 42 Task 2 (D-06): the Hermes enrollment is PROVEN by a uniquely-named test.
crates/devflow-core/src/agents/mod.rs:354:    /// Asserts the hardcoded array is now 6 drivers AND that the Hermes driver
crates/devflow-core/src/agents/mod.rs:357:    fn hermes_conformance_enrollment() {
crates/devflow-core/src/agents/mod.rs:364:            Box::new(HermesDriver),
crates/devflow-core/src/agents/mod.rs:366:        let hermes = drivers
crates/devflow-core/src/agents/mod.rs:368:            .find(|d| d.name() == "Hermes")
crates/devflow-core/src/agents/mod.rs:369:            .expect("the Hermes driver must be enrolled in the shared suite");
crates/devflow-core/src/agents/mod.rs:370:        let results = hermes.test_contract();
crates/devflow-core/src/agents/mod.rs:379:                "Hermes failed conformance case {:?}",
crates/devflow-cli/src/pipeline_gate.rs:311:/// the long poll timeout. Acks the response so the Hermes poller can clean up.
crates/devflow-cli/src/pipeline_gate.rs:470:/// Guard order (D-02, review: Codex HIGH + MEDIUM, Hermes ack-race):
crates/devflow-cli/src/pipeline_gate.rs:485:/// auditability (Hermes LOW: make `--force` semantics explicit), but is
crates/devflow-cli/src/pipeline_gate.rs:1694:    /// 20e Task 2 (review: Hermes ack-race): a Ship response that already
crates/devflow-cli/src/commands.rs:1975:        "Cron instruction pending (phase {}): hermes cron create --from-devflow {}",
crates/devflow-cli/src/commands.rs:2339:        // Phase 42 Task 3 (HRMS-01, D-06): presence-only probe of the `hermes` binary.
crates/devflow-cli/src/commands.rs:2341:            "hermes",
crates/devflow-cli/src/commands.rs:2342:            "hermes",
crates/devflow-cli/src/commands.rs:2344:            "Install the Hermes Agent CLI so `hermes` is on PATH",
crates/devflow-cli/src/commands.rs:4083:    fn cron_instruction_hints_include_hermes_command_per_phase() {
crates/devflow-cli/src/commands.rs:4086:        // the base hermes-command hint from 21a's reset-time fragment
crates/devflow-cli/src/commands.rs:4100:                "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
crates/devflow-cli/src/commands.rs:4119:            "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
crates/devflow-cli/src/commands.rs:4139:                "Cron instruction pending (phase 7): hermes cron create --from-devflow {}",
crates/devflow-cli/src/commands.rs:6840:    fn doctor_includes_hermes_check_in_the_seam() {
crates/devflow-cli/src/commands.rs:6842:        let hermes = checks
crates/devflow-cli/src/commands.rs:6844:            .find(|c| c.name == "hermes")
crates/devflow-cli/src/commands.rs:6845:            .expect("doctor_checks() must contain the hermes entry");
crates/devflow-cli/src/commands.rs:6846:        if hermes.status == "missing" {
crates/devflow-cli/src/commands.rs:6848:                hermes.install_hint.as_deref().unwrap_or("").contains("hermes"),
crates/devflow-cli/src/commands.rs:6849:                "the hint must name the hermes binary: {:?}",
crates/devflow-cli/src/commands.rs:6850:                hermes.install_hint

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "rg -n \"capabilities\\(\\)|health\\(|health_classification|discover\\(\" crates/devflow-cli crates/devflow-core/src | head -120; nl -ba crates/devflow-core/src/agents/mod.rs | sed -n '270,410p'" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 465ms:
crates/devflow-core/src/agents/hermes.rs:56:    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
crates/devflow-core/src/agents/pi.rs:66:    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
crates/devflow-core/src/agents/pi.rs:346:            .health(&test_state())
crates/devflow-core/src/agents/pi.rs:371:            .health(&test_state())
crates/devflow-core/src/agents/pi.rs:389:            PiDriver.health(&test_state()).is_err(),
crates/devflow-core/src/agents/pi.rs:406:            .health(&test_state())
crates/devflow-core/src/agents/pi.rs:426:        assert!(PiDriver.capabilities().subagent_dispatch);
crates/devflow-core/src/agents/pi.rs:442:        assert!(!PiDriver.capabilities().subagent_dispatch);
crates/devflow-core/src/agents/pi.rs:452:        assert!(!PiDriver.capabilities().subagent_dispatch);
crates/devflow-core/src/agents/pi.rs:462:        assert!(!PiDriver.capabilities().subagent_dispatch);
crates/devflow-core/src/agents/mod.rs:97:    fn health(&self, _state: &crate::state::State) -> Result<(), String> {
crates/devflow-core/src/agents/mod.rs:112:    fn discover(&self) -> Result<(), String> {
crates/devflow-core/src/agents/mod.rs:135:    fn health_classification(&self, state: &crate::state::State) -> DriverHealth {
crates/devflow-core/src/agents/mod.rs:136:        match self.health(state) {
crates/devflow-core/src/agents/mod.rs:638:        assert!(driver_for(AgentKind::Claude).health(&state).is_ok());
crates/devflow-core/src/agents/mod.rs:639:        assert!(driver_for(AgentKind::Codex).health(&state).is_ok());
crates/devflow-core/src/agents/mod.rs:640:        assert!(driver_for(AgentKind::OpenCode).health(&state).is_ok());
crates/devflow-cli/src/test_support.rs:217:    fn health(&self, _state: &State) -> Result<(), String> {
crates/devflow-cli/src/test_support.rs:256:    fn health(&self, _state: &State) -> Result<(), String> {
crates/devflow-cli/src/preflight.rs:1289:        generic_preflight_checks(project_root, state).and_then(|()| driver.health(state))
crates/devflow-cli/src/commands.rs:2473:        .capabilities()
crates/devflow-cli/src/commands.rs:3753:    /// the probe itself is covered by `PiDriver::capabilities()` tests
   270	        let (program, args) = PiDriver.build_command(PhaseId::new(7), "x", &[]);
   271	        assert_eq!(program, "pi");
   272	        assert_eq!(args, ["-p", "--no-approve", "x"]);
   273	        assert!(PiDriver.render_prompt(&intent).contains("execute-phase.md"));
   274	        assert!(
   275	            !PiDriver
   276	                .render_prompt(&intent)
   277	                .contains("/gsd-execute-phase")
   278	        );
   279	    }
   280	
   281	    /// 37-04: every driver passes the shared conformance suite, and Codex
   282	    /// declares the Define/Plan interactivity requirement that replaces the
   283	    /// hardcoded Codex-Define check.
   284	    #[test]
   285	    fn every_driver_passes_the_conformance_suite() {
   286	        let drivers: [Box<dyn AgentDriver>; 6] = [
   287	            Box::new(ClaudeDriver),
   288	            Box::new(CodexDriver),
   289	            Box::new(OpenCodeDriver),
   290	            Box::new(PiDriver),
   291	            Box::new(AntigravityDriver),
   292	            Box::new(HermesDriver),
   293	        ];
   294	        for driver in &drivers {
   295	            let results = driver.test_contract();
   296	            assert!(
   297	                !results.is_empty(),
   298	                "{} has no conformance cases",
   299	                driver.name()
   300	            );
   301	            for result in &results {
   302	                assert!(
   303	                    result.passed,
   304	                    "{} failed conformance case {:?}",
   305	                    driver.name(),
   306	                    result.name
   307	                );
   308	            }
   309	        }
   310	    }
   311	
   312	    /// F6: the Antigravity enrollment is PROVEN by a uniquely-named test —
   313	    /// the generic `conformance` filter matched two pre-existing tests and
   314	    /// would pass with zero Antigravity code, so the enrollment needed a name
   315	    /// that can only match this one. Asserts the hardcoded array is now 5
   316	    /// drivers AND that the Antigravity driver passes all 7 contract checks.
   317	    #[test]
   318	    fn antigravity_conformance_enrollment() {
   319	        let drivers: [Box<dyn AgentDriver>; 5] = [
   320	            Box::new(ClaudeDriver),
   321	            Box::new(CodexDriver),
   322	            Box::new(OpenCodeDriver),
   323	            Box::new(PiDriver),
   324	            Box::new(AntigravityDriver),
   325	        ];
   326	        let antigravity = drivers
   327	            .iter()
   328	            .find(|d| d.name() == "Antigravity")
   329	            .expect("the Antigravity driver must be enrolled in the shared suite");
   330	        let results = antigravity.test_contract();
   331	        assert_eq!(
   332	            results.len(),
   333	            7,
   334	            "1 name + 5 per-stage DEVFLOW_RESULT prompts + 1 program"
   335	        );
   336	        for result in &results {
   337	            assert!(
   338	                result.passed,
   339	                "Antigravity failed conformance case {:?}",
   340	                result.name
   341	            );
   342	        }
   343	        // The whole suite still passes with the 5th driver present.
   344	        for driver in &drivers {
   345	            assert!(
   346	                driver.test_contract().iter().all(|r| r.passed),
   347	                "{} must pass the shared conformance suite",
   348	                driver.name()
   349	            );
   350	        }
   351	    }
   352	
   353	    /// Phase 42 Task 2 (D-06): the Hermes enrollment is PROVEN by a uniquely-named test.
   354	    /// Asserts the hardcoded array is now 6 drivers AND that the Hermes driver
   355	    /// passes all 7 contract checks.
   356	    #[test]
   357	    fn hermes_conformance_enrollment() {
   358	        let drivers: [Box<dyn AgentDriver>; 6] = [
   359	            Box::new(ClaudeDriver),
   360	            Box::new(CodexDriver),
   361	            Box::new(OpenCodeDriver),
   362	            Box::new(PiDriver),
   363	            Box::new(AntigravityDriver),
   364	            Box::new(HermesDriver),
   365	        ];
   366	        let hermes = drivers
   367	            .iter()
   368	            .find(|d| d.name() == "Hermes")
   369	            .expect("the Hermes driver must be enrolled in the shared suite");
   370	        let results = hermes.test_contract();
   371	        assert_eq!(
   372	            results.len(),
   373	            7,
   374	            "1 name + 5 per-stage DEVFLOW_RESULT prompts + 1 program"
   375	        );
   376	        for result in &results {
   377	            assert!(
   378	                result.passed,
   379	                "Hermes failed conformance case {:?}",
   380	                result.name
   381	            );
   382	        }
   383	        // The whole suite still passes with the 6th driver present.
   384	        for driver in &drivers {
   385	            assert!(
   386	                driver.test_contract().iter().all(|r| r.passed),
   387	                "{} must pass the shared conformance suite",
   388	                driver.name()
   389	            );
   390	        }
   391	    }
   392	
   393	    /// A deliberately-broken driver: empty render + empty program. The suite
   394	    /// must FAIL it — the negative control proving `test_contract` isn't
   395	    /// vacuous (code-review finding #7).
   396	    struct BrokenDriver;
   397	
   398	    impl AgentDriver for BrokenDriver {
   399	        fn name(&self) -> &'static str {
   400	            "broken"
   401	        }
   402	        fn render_prompt(&self, _intent: &crate::prompt::StageIntent) -> String {
   403	            String::new()
   404	        }
   405	        fn build_command(
   406	            &self,
   407	            _phase: PhaseId,
   408	            _prompt: &str,
   409	            _roots: &[PathBuf],
   410	        ) -> (&'static str, Vec<String>) {

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "nl -ba crates/devflow-cli/src/commands.rs | sed -n '2440,2490p'; nl -ba crates/devflow-cli/src/preflight.rs | sed -n '1270,1300p'" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 14ms:
  1270	/// been evaluated and its reason shown to the human before the gate opens,
  1271	/// so skipping the re-check on `Advance` cannot silently skip a check that
  1272	/// never ran once.** The `Advance` arm therefore relaunches via
  1273	/// [`launch_stage_inner`] directly, SKIPPING this function entirely on the
  1274	/// retry. `GateAction::LoopBack` still calls the full [`launch_stage`]
  1275	/// (re-entering this function), because that path means the operator will
  1276	/// fix the condition and retry, and the state may genuinely have changed.
  1277	/// Either arm's recursion is bounded by `state.preflight_retries` /
  1278	/// [`mode::MAX_PREFLIGHT_RETRIES`]: the ceiling is checked BEFORE writing
  1279	/// another gate, so reaching it aborts with a logged
  1280	/// `preflight_retry_ceiling_reached` event instead of polling a second
  1281	/// 7-day gate timeout nobody will ever answer (T-18-27, T-18-30).
  1282	pub(crate) fn run_preflight(
  1283	    project_root: &Path,
  1284	    state: &mut State,
  1285	    driver: &dyn agents::AgentDriver,
  1286	) -> Result<bool, CliError> {
  1287	    let stage = state.stage;
  1288	    if let Err(reason) =
  1289	        generic_preflight_checks(project_root, state).and_then(|()| driver.health(state))
  1290	    {
  1291	        // Check the ceiling BEFORE writing another gate — writing the gate
  1292	        // first would let the ceiling case open yet another gate nobody
  1293	        // will answer (T-18-27).
  1294	        if state.preflight_retries >= mode::MAX_PREFLIGHT_RETRIES {
  1295	            let ceiling_reason = format!(
  1296	                "preflight retry ceiling ({}) reached for stage {stage}: {}",
  1297	                mode::MAX_PREFLIGHT_RETRIES,
  1298	                truncate_reason(&reason)
  1299	            );
  1300	            events::emit(

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc 'cat .planning/phases/42-hermes-driver/42-VERIFICATION.md; cat .planning/phases/42-hermes-driver/42-UAT.md' in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 0ms:
# Phase 42: Hermes Driver & Antigravity Dogfood Verification

## Execution Summary

- **Phase**: 42 (Hermes Agent Driver & Supervised Antigravity Dogfood Execution)
- **Agent**: Antigravity (`agy`, stream-json transport, `--print-timeout 60m`)
- **Mode**: Supervised (`--mode supervise`)
- **Deliverables**:
  1. `crates/devflow-core/src/agents/hermes.rs`: Modular `HermesDriver` implementing `AgentDriver` with headless `-z` oneshot launch, `HERMES_ACCEPT_HOOKS=1`, claude-style prompt rendering, dynamic `hermes tools list` delegation probe, and presence-only health check.
  2. `crates/devflow-core/src/state.rs`: Registered `AgentKind::Hermes` variant, serde roundtrip, case-insensitive parser, and Display formatting.
  3. `crates/devflow-core/src/agents/mod.rs`: Driver dispatch wiring and 6-driver conformance suite enrollment (`every_driver_passes_the_conformance_suite` and `hermes_conformance_enrollment`).
  4. `crates/devflow-cli/src/commands.rs`: Added `hermes` cmd_check in `doctor_checks()` and `doctor_includes_hermes_check_in_the_seam` unit test.
  5. `crates/devflow-cli/tests/phase7_cli.rs`: Integration regressions (`hermes_marker_less_run_does_not_advance`, `hermes_nonzero_exit_does_not_advance`, `hermes_hung_process_is_detected_not_left_running`) with `MonitorReapGuard`.
  6. `crates/devflow-cli/src/preflight.rs`: Unlocked `--mode auto` for Antigravity in `unattended_launch_shape_condition` following successful supervised dogfood run.

---

## Dogfood Cadence & Quiet-Gap Measurement (ANTG-04)

- **Idle Timeout Floor**: 120 seconds (`DEVFLOW_ANTIGRAVITY_IDLE_TIMEOUT_SECS`)
- **Print Timeout Override**: `--print-timeout 60m`
- **Observed Cadence**:
  - Stream events emitted regularly during tool dispatches, file reads, and shell executions.
  - Quiet gaps between events remained within bounds; no false-alarm idle timeout was observed.
  - The 60m print-timeout override held continuously across multi-minute compilation and test suite passes without termination.

---

## Automated Test Verification

| Test Suite | Command | Result |
|------------|---------|--------|
| Hermes Unit Tests | `cargo test -p devflow-core --lib hermes` | 14 passed; 0 failed |
| AgentKind Tests | `cargo test -p devflow-core --lib agent_kind_hermes` | 5 passed; 0 failed |
| Conformance Suite | `cargo test -p devflow-core --lib hermes_conformance_enrollment` | 1 passed (6 drivers passing); 0 failed |
| Doctor Check | `cargo test -p devflow --bin devflow doctor_includes_hermes` | 1 passed; 0 failed |
| Transport Integration | `cargo test -p devflow --test phase7_cli hermes` | 3 passed; 0 failed |
| Preflight C2 Unlock | `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity` | 1 passed; 0 failed |
| Full Workspace Suite | `cargo test --workspace` | >1,000 passed; 0 failed |
---
status: passed
phase: 42-hermes-driver
source: [42-01-PLAN.md, 42-02-PLAN.md, 42-VALIDATION.md, 42-VERIFICATION.md]
started: 2026-08-21T21:00:00Z
updated: 2026-08-21T21:30:00Z
---

## Current Test

number: 1
name: Phase 42 automated verification & UAT sign-off set
expected: |
  All Hermes driver unit tests, conformance checks, doctor checks, transport
  integration regressions, and preflight C2 auto-mode unlock tests pass with zero
  failures. The full workspace suite passes cleanly.

## Tests

### 1. HermesDriver modular implementation (HRMS-01, D-01, D-02, D-04)
result: passed
evidence: `cargo test -p devflow-core --lib hermes` → 14 passed (command shape, environment `HERMES_ACCEPT_HOOKS=1`, prompt rendering, and delegation probing).

### 2. AgentKind registration & 6-driver conformance enrollment (HRMS-01, D-05, D-06)
result: passed
evidence: `cargo test -p devflow-core --lib agent_kind_hermes` (5 passed), `hermes_conformance_enrollment` (1 passed, all 6 drivers pass 7 contract checks).

### 3. devflow doctor presence probe (HRMS-01, D-06)
result: passed
evidence: `cargo test -p devflow --bin devflow doctor_includes_hermes` → 1 passed.

### 4. Transport integration regressions with MonitorReapGuard (HRMS-03, D-03)
result: passed
evidence: `cargo test -p devflow --test phase7_cli hermes` → 3 passed (`hermes_marker_less_run_does_not_advance`, `hermes_nonzero_exit_does_not_advance`, `hermes_hung_process_is_detected_not_left_running`).

### 5. Antigravity supervised dogfooding & cadence verification (ANTG-04, D-07)
result: passed
evidence: Supervised execution completed without false idle timeouts; 60m print-timeout override held across long compilation steps.

### 6. Unattended mode unlocked for Antigravity in preflight (ANTG-04, D-07)
result: passed
evidence: `cargo test -p devflow --bin devflow unattended_launch_shape_condition_antigravity` → 1 passed.

### 7. Full workspace regression test suite
result: passed
evidence: `cargo test --workspace` → >1,000 passed; 0 failed.

## Summary

total: 7
passed: 7
issues: 0
pending: 0
skipped: 0
blocked: 0

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "rg -n \"fn hermes_|hermes_marker|hermes_nonzero|hermes_hung|Hermes\" crates/devflow-cli/tests/phase7_cli.rs; nl -ba crates/devflow-cli/tests/phase7_cli.rs | sed -n '1780,1970p'" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 201ms:
1790:// Phase 42 Task 4 (HRMS-03, D-03): Hermes transport integration tests.
1793:fn hermes_stub(launch: &str) -> String {
1812:fn hermes_marker_less_run_does_not_advance() {
1842:/// HRMS-03: a Hermes run that exits non-zero must not advance its stage.
1844:fn hermes_nonzero_exit_does_not_advance() {
1874:/// HRMS-03: a hung Hermes process is detected as alive by monitor liveness,
1877:fn hermes_hung_process_is_detected_not_left_running() {
  1780	    assert!(
  1781	        alive.contains(&pid),
  1782	        "a live registered monitor must be detected: {alive:?}"
  1783	    );
  1784	
  1785	    // Clean up through the normal guard: verified reap + deregister.
  1786	    let _reap = MonitorReapGuard::after_launch(&state);
  1787	}
  1788	
  1789	// ---------------------------------------------------------------------------
  1790	// Phase 42 Task 4 (HRMS-03, D-03): Hermes transport integration tests.
  1791	// ---------------------------------------------------------------------------
  1792	
  1793	fn hermes_stub(launch: &str) -> String {
  1794	    format!(
  1795	        r#"#!/bin/sh
  1796	if [ "$1" = "--version" ]; then
  1797	    printf 'hermes 0.5.0\n'
  1798	    exit 0
  1799	fi
  1800	if [ "$1" = "tools" ] && [ "$2" = "list" ]; then
  1801	    printf 'Available Toolsets:\n  ✓ enabled delegation 👥 Task Delegation\n  ✓ enabled terminal 💻 Terminal Execution\n'
  1802	    exit 0
  1803	fi
  1804	{launch}
  1805	"#
  1806	    )
  1807	}
  1808	
  1809	/// HRMS-03: a stubbed `hermes` that exits 0 with no marker must not advance a
  1810	/// commit-gated stage. Define advances on exit 0; Plan gates.
  1811	#[test]
  1812	fn hermes_marker_less_run_does_not_advance() {
  1813	    let repo = tempfile::tempdir().unwrap();
  1814	    let root = repo.path();
  1815	    init_repo(root);
  1816	    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("printf 'fake hermes, no marker\\n'\nexit 0\n"))]);
  1817	
  1818	    run_devflow(
  1819	        root,
  1820	        &fake_bin.path,
  1821	        &[
  1822	            "start",
  1823	            "--phase",
  1824	            "07",
  1825	            "--agent",
  1826	            "hermes",
  1827	            "--mode",
  1828	            "supervise",
  1829	        ],
  1830	    );
  1831	
  1832	    let state = wait_for_gate(root, PhaseId::new(7));
  1833	    assert_eq!(
  1834	        state.stage,
  1835	        Stage::Plan,
  1836	        "a marker-less hermes run must not advance past the commit-gated Plan stage"
  1837	    );
  1838	    assert!(state.gate_pending, "the never-silent gate must have fired");
  1839	    let _reap = MonitorReapGuard::after_launch(&state);
  1840	}
  1841	
  1842	/// HRMS-03: a Hermes run that exits non-zero must not advance its stage.
  1843	#[test]
  1844	fn hermes_nonzero_exit_does_not_advance() {
  1845	    let repo = tempfile::tempdir().unwrap();
  1846	    let root = repo.path();
  1847	    init_repo(root);
  1848	    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("printf 'fake hermes error\\n'\nexit 1\n"))]);
  1849	
  1850	    run_devflow(
  1851	        root,
  1852	        &fake_bin.path,
  1853	        &[
  1854	            "start",
  1855	            "--phase",
  1856	            "07",
  1857	            "--agent",
  1858	            "hermes",
  1859	            "--mode",
  1860	            "supervise",
  1861	        ],
  1862	    );
  1863	
  1864	    let state = wait_for_gate(root, PhaseId::new(7));
  1865	    assert_eq!(
  1866	        state.stage,
  1867	        Stage::Define,
  1868	        "a non-zero-exit hermes run must not advance its stage"
  1869	    );
  1870	    assert!(state.gate_pending, "the never-silent gate must have fired");
  1871	    let _reap = MonitorReapGuard::after_launch(&state);
  1872	}
  1873	
  1874	/// HRMS-03: a hung Hermes process is detected as alive by monitor liveness,
  1875	/// does not falsely advance, and gates when killed.
  1876	#[test]
  1877	fn hermes_hung_process_is_detected_not_left_running() {
  1878	    let repo = tempfile::tempdir().unwrap();
  1879	    let root = repo.path();
  1880	    init_repo(root);
  1881	    let fake_bin = fake_bin_dir(&[("hermes", &hermes_stub("exec sleep 30\n"))]);
  1882	
  1883	    run_devflow(
  1884	        root,
  1885	        &fake_bin.path,
  1886	        &[
  1887	            "start",
  1888	            "--phase",
  1889	            "07",
  1890	            "--agent",
  1891	            "hermes",
  1892	            "--mode",
  1893	            "supervise",
  1894	        ],
  1895	    );
  1896	
  1897	    let pid_path = root.join(".devflow/phase-07-agent-pid");
  1898	    let pid = wait_for_pid(&pid_path);
  1899	
  1900	    assert!(
  1901	        Command::new("kill")
  1902	            .arg("-0")
  1903	            .arg(pid.to_string())
  1904	            .status()
  1905	            .unwrap()
  1906	            .success(),
  1907	        "the hung hermes process must still be alive"
  1908	    );
  1909	    let state = load_state(root, PhaseId::new(7)).expect("load state");
  1910	    assert_eq!(
  1911	        state.stage,
  1912	        Stage::Define,
  1913	        "the stage must not advance while hermes is hung"
  1914	    );
  1915	
  1916	    assert!(
  1917	        Command::new("kill")
  1918	            .arg(pid.to_string())
  1919	            .status()
  1920	            .unwrap()
  1921	            .success(),
  1922	        "should be able to kill the hung hermes process"
  1923	    );
  1924	    let gated = wait_for_gate(root, PhaseId::new(7));
  1925	    assert_eq!(
  1926	        gated.stage,
  1927	        Stage::Define,
  1928	        "a hung-then-killed hermes run must not advance its stage"
  1929	    );
  1930	    let _reap = MonitorReapGuard::after_launch(&gated);
  1931	}

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc 'rg -n "CRITICAL|HIGH|MEDIUM|WARNING|Finding|Defect|false positive|Hermes" .planning/phases/42-hermes-driver/review_codex.md | head -120' in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 0ms:
13:You are an adversarial CODE reviewer. Attack the implementation of Phase 42 (Hermes Agent Driver & Antigravity Unattended Mode Graduation) in this repository. Do not summarize or praise the code. Your job is to find real, concrete defects with the exact file and line/quoted text for each finding.
28:- Goal: Implement modular HermesDriver for Hermes Agent CLI (`hermes -z "<prompt>" --yolo --accept-hooks`), with `HERMES_ACCEPT_HOOKS=1`, dynamic `hermes tools list` delegation probe, presence-only health probe, registration of AgentKind::Hermes, 6-driver conformance suite enrollment, devflow doctor presence probe, transport integration tests with MonitorReapGuard, and unlocking unattended auto mode for Antigravity in preflight.rs (C2 gate) following supervised dogfooding.
42:3. Defect explanation (why it is broken)
100:   - **CRITICAL** - Security vulnerability (must fix before merge)
101:   - **HIGH** - Bug or major code smell (should fix before merge)
102:   - **MEDIUM** - Minor issue (fix when possible)
151:- Issues by severity (CRITICAL, HIGH, MEDIUM, LOW)
217:CRITICAL (0)
221:HIGH (0)
225:MEDIUM (7)
315:     1	//! Hermes coding-agent adapter (Phase 42).
329:    15	/// The modular driver for Hermes (Phase 42): headless `-z` oneshot launch,
332:    18	pub struct HermesDriver;
334:    20	impl AgentDriver for HermesDriver {
336:    22	        "Hermes"
385:    71	/// Dynamically probe whether Hermes has the `delegation` toolset enabled.
430:   116	        let driver = HermesDriver;
431:   117	        assert_eq!(driver.name(), "Hermes");
436:   122	        let driver = HermesDriver;
452:   138	        let driver = HermesDriver;
459:   145	        let driver = HermesDriver;
663:   144	/// A future driver (Antigravity, Hermes) plugs in by passing these — the
700:   181	        AgentKind::Hermes => Box::new(HermesDriver),
714:   195	pub use hermes::HermesDriver;
731:   212	        assert_eq!(driver_for(AgentKind::Hermes).name(), "Hermes");
811:   292	            Box::new(HermesDriver),
872:   353	    /// Phase 42 Task 2 (D-06): the Hermes enrollment is PROVEN by a uniquely-named test.
873:   354	    /// Asserts the hardcoded array is now 6 drivers AND that the Hermes driver
883:   364	            Box::new(HermesDriver),
887:   368	            .find(|d| d.name() == "Hermes")
888:   369	            .expect("the Hermes driver must be enrolled in the shared suite");
898:   379	                "Hermes failed conformance case {:?}",
1559:   398	    /// Hermes Agent CLI (`hermes`), headless oneshot (phase 42).
1560:   399	    Hermes,
1571:   410	            AgentKind::Hermes => "hermes",
1587:   426	            "hermes" => Ok(AgentKind::Hermes),
2290:  1129	    // Phase 42 Task 2: AgentKind::Hermes (HRMS-01, D-05, D-06).
2295:  1134	        assert_eq!("hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
2296:  1135	        assert_eq!("HERMES".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
2297:  1136	        assert_eq!("Hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
2302:  1141	        assert_eq!(AgentKind::Hermes.to_string(), "hermes");
2307:  1146	        let wire = serde_json::to_string(&AgentKind::Hermes).unwrap();
2310:  1149	        assert_eq!(back, AgentKind::Hermes);
2325:  1164	            crate::agents::driver_for(AgentKind::Hermes).name(),
2326:  1165	            "Hermes"
2336:/bin/bash -lc 'rg -n "Hermes|hermes|unattended_launch_shape_condition|MonitorReapGuard|test_contract|delegation|stop_until|AgentKind" crates/devflow-cli/src/commands.rs crates/devflow-cli/src/preflight.rs crates/devflow-cli/tests/phase7_cli.rs crates/devflow-core/src/state.rs .planning/phases/42-hermes-driver/*.md' in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
2344:crates/devflow-core/src/state.rs:398:    /// Hermes Agent CLI (`hermes`), headless oneshot (phase 42).
2345:crates/devflow-core/src/state.rs:399:    Hermes,
2352:crates/devflow-core/src/state.rs:410:            AgentKind::Hermes => "hermes",
2359:crates/devflow-core/src/state.rs:426:            "hermes" => Ok(AgentKind::Hermes),
2415:crates/devflow-core/src/state.rs:1129:    // Phase 42 Task 2: AgentKind::Hermes (HRMS-01, D-05, D-06).
2417:crates/devflow-core/src/state.rs:1134:        assert_eq!("hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
2418:crates/devflow-core/src/state.rs:1135:        assert_eq!("HERMES".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
2419:crates/devflow-core/src/state.rs:1136:        assert_eq!("Hermes".parse::<AgentKind>().unwrap(), AgentKind::Hermes);
2421:crates/devflow-core/src/state.rs:1141:        assert_eq!(AgentKind::Hermes.to_string(), "hermes");
2423:crates/devflow-core/src/state.rs:1146:        let wire = serde_json::to_string(&AgentKind::Hermes).unwrap();
2426:crates/devflow-core/src/state.rs:1149:        assert_eq!(back, AgentKind::Hermes);
2431:crates/devflow-core/src/state.rs:1164:            crate::agents::driver_for(AgentKind::Hermes).name(),
2432:crates/devflow-core/src/state.rs:1165:            "Hermes"
2488:.planning/phases/42-hermes-driver/42-02-PLAN.md:67:    - `hermes --version` exits 0 and reports Hermes Agent
2489:.planning/phases/42-hermes-driver/42-02-PLAN.md:68:    - `devflow doctor` reports both Antigravity and Hermes present
2537:crates/devflow-cli/tests/phase7_cli.rs:1790:// Phase 42 Task 4 (HRMS-03, D-03): Hermes transport integration tests.
2547:crates/devflow-cli/tests/phase7_cli.rs:1842:/// HRMS-03: a Hermes run that exits non-zero must not advance its stage.
2553:crates/devflow-cli/tests/phase7_cli.rs:1874:/// HRMS-03: a hung Hermes process is detected as alive by monitor liveness,
2562:.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:1:# Phase 42: Hermes Driver - Discussion Log
2564:.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:8:**Areas discussed:** Hermes CLI argv & flags, Prompt rendering style, Subagent dispatch capabilities, Antigravity dogfooding execution
2565:.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:12:## Hermes CLI Argv & Flags
2572:.planning/phases/42-hermes-driver/42-DISCUSSION-LOG.md:44:**Notes:** Hermes CLI includes a built-in `delegation` toolset (`✓ enabled delegation 👥 Task Delegation`). Implemented dynamic probe helper mirroring `pi_subagent_dispatch_available`.
2583:crates/devflow-cli/src/commands.rs:2344:            "Install the Hermes Agent CLI so `hermes` is on PATH",
2627:.planning/phases/42-hermes-driver/42-01-PLAN.md:25:    - "`HermesDriver::build_command` returns `(\"hermes\", vec![\"-z\".into(), prompt.to_string(), \"--yolo\".into(), \"--accept-hooks\".into()])` and `environment()` sets `HERMES_ACCEPT_HOOKS=1` (HRMS-02, D-01)"
2628:.planning/phases/42-hermes-driver/42-01-PLAN.md:26:    - "`HermesDriver::render_prompt` delegates to `crate::prompt::render_claude_style(intent)` (D-02)"
2629:.planning/phases/42-hermes-driver/42-01-PLAN.md:27:    - "`HermesDriver::capabilities` probes `hermes tools list` for enabled delegation via `hermes_subagent_dispatch_available()` (D-04)"
2630:.planning/phases/42-hermes-driver/42-01-PLAN.md:28:    - "HermesDriver is enrolled in the shared conformance suite (5 → 6 drivers), proven by the `hermes_conformance_enrollment` unit test (D-06)"
2633:.planning/phases/42-hermes-driver/42-01-PLAN.md:32:    - `crates/devflow-core/src/agents/hermes.rs` — HermesDriver implementation, environment, subagent capability probe, and unit tests
2634:.planning/phases/42-hermes-driver/42-01-PLAN.md:33:    - `crates/devflow-core/src/state.rs` — `AgentKind::Hermes` variant, `Display`, `FromStr`, and serde serialization
2637:.planning/phases/42-hermes-driver/42-01-PLAN.md:40:Implement the modular Hermes agent driver (`AgentKind::Hermes`), register it across `devflow-core` and `devflow-cli`, enroll it in the shared conformance test suite, add presence-only health checks to `devflow doctor`, and verify process-exit failure modes (marker-less, non-zero exit, hung process) via integration tests.
2638:.planning/phases/42-hermes-driver/42-01-PLAN.md:42:Purpose: Deliver full Hermes agent support in DevFlow (HRMS-01, HRMS-02, HRMS-03) matching the modular driver contract.
2639:.planning/phases/42-hermes-driver/42-01-PLAN.md:43:Output: Working `HermesDriver` module, updated `state.rs`, `agents/mod.rs`, `commands.rs`, and green unit/integration tests.
2643:.planning/phases/42-hermes-driver/42-01-PLAN.md:69:  <name>Task 1: Implement HermesDriver in crates/devflow-core/src/agents/hermes.rs</name>
2646:.planning/phases/42-hermes-driver/42-01-PLAN.md:79:    - `crates/devflow-core/src/agents/hermes.rs` exists and defines `pub struct HermesDriver` implementing `AgentDriver`
2647:.planning/phases/42-hermes-driver/42-01-PLAN.md:80:    - `HermesDriver::name()` returns `"Hermes"`
2648:.planning/phases/42-hermes-driver/42-01-PLAN.md:81:    - `HermesDriver::render_prompt()` delegates to `crate::prompt::render_claude_style(intent)` (D-02)
2649:.planning/phases/42-hermes-driver/42-01-PLAN.md:82:    - `HermesDriver::build_command()` returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])` (D-01)
2650:.planning/phases/42-hermes-driver/42-01-PLAN.md:83:    - `HermesDriver::environment()` returns `vec![("HERMES_ACCEPT_HOOKS".into(), "1".into())]` (D-01)
2651:.planning/phases/42-hermes-driver/42-01-PLAN.md:84:    - `HermesDriver::capabilities()` returns `DriverCapabilities { subagent_dispatch: hermes_subagent_dispatch_available() }` (D-04)
2655:.planning/phases/42-hermes-driver/42-01-PLAN.md:90:    Create `crates/devflow-core/src/agents/hermes.rs` implementing `AgentDriver` for `HermesDriver`.
2659:.planning/phases/42-hermes-driver/42-01-PLAN.md:97:  <done>HermesDriver is implemented with unit tests and verified green.</done>
2660:.planning/phases/42-hermes-driver/42-01-PLAN.md:101:  <name>Task 2: Register AgentKind::Hermes and wire driver dispatch and conformance suite</name>
2662:.planning/phases/42-hermes-driver/42-01-PLAN.md:109:    - `crates/devflow-core/src/state.rs` enum `AgentKind` includes `Hermes` variant (D-05)
2663:.planning/phases/42-hermes-driver/42-01-PLAN.md:110:    - `AgentKind::Hermes.to_string()` returns `"hermes"`
2664:.planning/phases/42-hermes-driver/42-01-PLAN.md:111:    - `"hermes".parse::<AgentKind>()` returns `Ok(AgentKind::Hermes)`
2665:.planning/phases/42-hermes-driver/42-01-PLAN.md:112:    - `crates/devflow-core/src/agents/mod.rs` exposes `pub mod hermes;` and `pub use hermes::HermesDriver;`
2666:.planning/phases/42-hermes-driver/42-01-PLAN.md:113:    - `driver_for(AgentKind::Hermes)` returns `Box::new(HermesDriver)`
2667:.planning/phases/42-hermes-driver/42-01-PLAN.md:115:    - Test `hermes_conformance_enrollment` in `agents/mod.rs` tests `HermesDriver` against all 7 contract checks
2670:.planning/phases/42-hermes-driver/42-01-PLAN.md:120:    1. In `crates/devflow-core/src/state.rs`: Add `Hermes` variant to `AgentKind`, update `Display`, `FromStr`, `AgentParseError` message, and add unit tests for `AgentKind::Hermes` serialization and parsing.
2671:.planning/phases/42-hermes-driver/42-01-PLAN.md:121:    2. In `crates/devflow-core/src/agents/mod.rs`: Add `pub mod hermes;`, `pub use hermes::HermesDriver;`, wire `driver_for(AgentKind::Hermes) => Box::new(HermesDriver)`, update `every_driver_passes_the_conformance_suite` to include `Box::new(HermesDriver)` in its 6-element array, and add `hermes_conformance_enrollment` test.
2673:.planning/phases/42-hermes-driver/42-01-PLAN.md:126:  <done>AgentKind::Hermes is registered, driver_for is wired, and conformance enrollment is verified.</done>
2674:.planning/phases/42-hermes-driver/42-01-PLAN.md:130:  <name>Task 3: Add Hermes presence probe to devflow doctor</name>
2682:.planning/phases/42-hermes-driver/42-01-PLAN.md:148:  <done>devflow doctor includes Hermes presence check, verified by unit test.</done>
2683:.planning/phases/42-hermes-driver/42-01-PLAN.md:152:  <name>Task 4: Add Hermes integration regression tests in phase7_cli.rs</name>
2696:.planning/phases/42-hermes-driver/42-01-PLAN.md:177:  <done>Hermes transport integration tests are implemented and verified green.</done>
2697:.planning/phases/42-hermes-driver/42-01-PLAN.md:187:| DevFlow process monitor → Hermes CLI | Spawning `hermes -z` with `--yolo` and `--accept-hooks` executes commands non-interactively. |
2698:.planning/phases/42-hermes-driver/42-01-PLAN.md:188:| Hermes process stdout → `parse_marker_lines` | Stdout parsing must strictly validate DEVFLOW_RESULT JSON format before advancing commit-gated stages. |
2699:.planning/phases/42-hermes-driver/42-01-PLAN.md:197:| T-42-04 | Repudiation | Conformance suite | low | mitigate | Enroll HermesDriver in conformance suite to verify 7 trait contract checks. |
2706:.planning/phases/42-hermes-driver/42-01-PLAN.md:212:`AgentKind::Hermes` resolves end-to-end; `HermesDriver` implements the modular driver contract; conformance suite passes with 6 drivers; `devflow doctor` checks Hermes; marker-less, error exit, and hung process regressions pass in `phase7_cli.rs`.
2708:.planning/phases/42-hermes-driver/42-01-PLAN.md:220:- `HermesDriver` (struct in `crates/devflow-core/src/agents/hermes.rs`)
2709:.planning/phases/42-hermes-driver/42-01-PLAN.md:221:- `AgentKind::Hermes` (enum variant in `crates/devflow-core/src/state.rs`)
2716:.planning/phases/42-hermes-driver/42-CONTEXT.md:1:# Phase 42: Hermes Driver - Context
2717:.planning/phases/42-hermes-driver/42-CONTEXT.md:9:Phase 42 delivers the modular **Hermes Driver** (`AgentKind::Hermes`), enabling `devflow start --agent hermes` to launch Hermes in oneshot headless mode (`hermes -z "<prompt>" --yolo --accept-hooks`) with honest completion detection and conformance suite enrollment (HRMS-01, HRMS-02, HRMS-03).
2718:.planning/phases/42-hermes-driver/42-CONTEXT.md:18:### Hermes Driver Core & Launch
2719:.planning/phases/42-hermes-driver/42-CONTEXT.md:20:  `HermesDriver::build_command` returns `("hermes", vec!["-z".into(), prompt.to_string(), "--yolo".into(), "--accept-hooks".into()])`.
2720:.planning/phases/42-hermes-driver/42-CONTEXT.md:21:  `HermesDriver::environment` sets `("HERMES_ACCEPT_HOOKS".into(), "1".into())`.
2722:.planning/phases/42-hermes-driver/42-CONTEXT.md:24:  `HermesDriver::render_prompt` delegates to `crate::prompt::render_claude_style(intent)`, providing standard `/gsd-*` commands and the `DEVFLOW_RESULT` completion contract.
2723:.planning/phases/42-hermes-driver/42-CONTEXT.md:26:  `HermesDriver` uses the standard process-exit completion transport: stdout is scanned for `DEVFLOW_RESULT` JSON markers via `parse_marker_lines`. A marker-less run never advances a commit-gated stage (`Plan`, `Code`).
2724:.planning/phases/42-hermes-driver/42-CONTEXT.md:28:  `HermesDriver::capabilities` probes `hermes tools list` for `enabled.*delegation` via a helper `hermes_subagent_dispatch_available()`, setting `subagent_dispatch: true` when enabled and `false` otherwise (mirroring the `pi_subagent_dispatch_available` pattern).
2726:.planning/phases/42-hermes-driver/42-CONTEXT.md:32:  Add `AgentKind::Hermes`, wire `FromStr` / `Display`, `driver_for` mapping (`Box::new(HermesDriver)`), and `agent_program` (`"hermes"`).
2727:.planning/phases/42-hermes-driver/42-CONTEXT.md:34:  Add `hermes` to `devflow doctor` checks (`doctor_checks()` in `commands.rs`). Enroll `HermesDriver` in `every_driver_passes_the_conformance_suite` (5 → 6 drivers in `crates/devflow-core/src/agents/mod.rs`).
2728:.planning/phases/42-hermes-driver/42-CONTEXT.md:45:- Exact layout of unit and integration test fixtures for Hermes driver.
2730:.planning/phases/42-hermes-driver/42-CONTEXT.md:56:- `.planning/REQUIREMENTS.md` § "Hermes" (HRMS-01..03) & "Antigravity Dogfood + Cadence" (ANTG-04)
2731:.planning/phases/42-hermes-driver/42-CONTEXT.md:57:- `.planning/ROADMAP.md` § "Phase 42: Hermes Driver"

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc "sed -n '210,315p' .planning/phases/42-hermes-driver/review_codex.md" in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 0ms:
CODE REVIEW REPORT
==================

Files Reviewed: 8
Total Issues: 12
Architectural Status: WATCH

CRITICAL (0)
-----------
(none)

HIGH (0)
--------
(none)

MEDIUM (7)
----------
1. src/api/auth.ts:42
   Issue: Email normalization logic is duplicated instead of reusing the shared helper
   Risk: Validation rules can drift between authentication paths
   Fix: Route both paths through the shared normalization helper

2. src/components/UserProfile.tsx:89
   Issue: Derived permissions are recalculated on every render
   Risk: Avoidable work during profile refreshes
   Fix: Memoize the derived permissions list or compute it upstream

3. src/utils/validation.ts:15
   Issue: Form-layer and server-layer validation messages are defined separately
   Risk: User-facing validation guidance can become inconsistent
   Fix: Share one validation message helper across both call sites

LOW (5)
-------
...

ARCHITECTURE WATCHLIST
----------------------
- src/review/orchestrator.ts:88
  Concern: Review result synthesis relies on implicit ordering rather than an explicit blocker contract
  Status: WATCH
  Recommendation: Define deterministic merge gating before expanding reviewers

SYNTHESIS
---------
- code-reviewer recommendation: COMMENT
- architect status: WATCH
- final recommendation: COMMENT

RECOMMENDATION: COMMENT

Address any WATCH concerns before treating the change as merge-ready.
```

## Review Checklist

The `code-reviewer` lane checks:

### Security
- [ ] No hardcoded secrets (API keys, passwords, tokens)
- [ ] All user inputs sanitized
- [ ] SQL/NoSQL injection prevention
- [ ] XSS prevention (escaped outputs)
- [ ] CSRF protection on state-changing operations
- [ ] Authentication/authorization properly enforced

### Code Quality
- [ ] Functions < 50 lines (guideline)
- [ ] Cyclomatic complexity < 10
- [ ] No deeply nested code (> 4 levels)
- [ ] No duplicate logic (DRY principle)
- [ ] Clear, descriptive naming

### Performance
- [ ] No N+1 query patterns
- [ ] Appropriate caching where applicable
- [ ] Efficient algorithms (avoid O(n²) when O(n) possible)
- [ ] No unnecessary re-renders (React/Vue)

### Best Practices
- [ ] Error handling present and appropriate
- [ ] Logging at appropriate levels
- [ ] Documentation for public APIs
- [ ] Tests for critical paths
- [ ] No commented-out code


hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
2026-08-21T21:41:27.961686Z ERROR codex_core::tools::router: error=Full-history forked agents inherit the parent agent type; omit agent_type, or spawn without a full-history fork.
hook: PreToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
exec
/bin/bash -lc 'git diff --stat && nl -ba crates/devflow-core/src/agents/hermes.rs && nl -ba crates/devflow-core/src/agents/mod.rs && nl -ba crates/devflow-core/src/state.rs' in /var/home/denniyahh/Github/devflow/.worktrees/phase-42
 succeeded in 122ms:
     1	//! Hermes coding-agent adapter (Phase 42).

hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
2026-08-21T21:43:07.287564Z ERROR codex_core::tools::router: error=timeout_ms must be at least 10000
hook: PreToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
collab: Wait
hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
collab: Wait
hook: PostToolUse
hook: PostToolUse Completed
hook: PreToolUse
hook: PreToolUse Completed
collab: Wait
hook: PostToolUse
hook: PostToolUse Completed
codex
CODE REVIEW REPORT — REQUEST CHANGES

Architectural status: BLOCK

### Warning

1. **Antigravity bypasses the legacy opt-out**

   **File:** [preflight.rs:981-985](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-cli/src/preflight.rs:981)

   ```rust
   if stream_launch_enabled(state.agent, Stage::Code, state.legacy_claude_launch)
       && (state.agent == AgentKind::Claude || state.agent == AgentKind::Antigravity)
   {
       return ConditionState::Holds;
   }
   ```

   `stream_launch_enabled` ignores `legacy_claude_launch` for Antigravity, so an Antigravity state with the legacy opt-out set still returns `Holds`.

   **Reproduction:** `AgentKind::Antigravity`, `Mode::Auto`, `legacy_claude_launch = true` → unattended launch is permitted. The existing tests cover Claude with the opt-out, but not Antigravity with it.

2. **The auto-mode unlock has no runtime dogfood proof**

   **File:** [preflight.rs:981-985](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-cli/src/preflight.rs:981)

   The predicate permits a brand-new `State::new(..., AgentKind::Antigravity, Mode::Auto, ...)` without any persisted evidence that supervised dogfooding occurred. This contradicts the nearby comment at lines 974-980, which says Antigravity should remain refused until a real dogfood run.

   **Reproduction:** The existing `unattended_launch_shape_condition_antigravity_allowed` fixture constructs a fresh state and immediately gets `Holds`.

3. **Delegation parsing accepts disabled toolsets**

   **File:** [hermes.rs:97-101](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-core/src/agents/hermes.rs:97)

   ```rust
   let lower = line.to_ascii_lowercase();
   if lower.contains("delegation") && lower.contains("enabled") {
       return true;
   }
   ```

   This is substring co-occurrence, not status parsing.

   **Reproduction:**

   ```text
   ✗ disabled delegation (enabled terminal tools only)
   ```

   returns `true`, advertising subagent dispatch when delegation is disabled. The negative test only places `disabled` and `enabled` on separate lines.

4. **Hermes probes can hang indefinitely**

   **File:** [hermes.rs:58-60](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-core/src/agents/hermes.rs:58)

   ```rust
   Command::new("hermes").arg("--version").output()
   ```

   **File:** [hermes.rs:76-79](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-core/src/agents/hermes.rs:76)

   ```rust
   Command::new("hermes").args(["tools", "list"]).output()
   ```

   Both synchronously wait without a timeout.

   **Reproduction:** Put an executable named `hermes` earlier on `PATH` containing `sleep 1d`; doctor or capability discovery never returns.

5. **Graduation evidence is not reproducible**

   **Files:** [42-VERIFICATION.md:18-25](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/.planning/phases/42-hermes-driver/42-VERIFICATION.md:18), [42-UAT.md:18-54](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/.planning/phases/42-hermes-driver/42-UAT.md:18)

   The artifacts claim bounded quiet gaps and a held 60-minute timeout, but provide no timestamps, sample count, maximum gap, command transcript, or raw timeout evidence. The single `ConditionState::Holds` unit test cannot establish that supervised dogfooding happened or that the timeout margin is safe.

### Info

6. **The conformance suite does not test Hermes’ launch contract**

   **File:** [agents/mod.rs:146-169](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-core/src/agents/mod.rs:146)

   ```rust
   passed: prompt.contains("DEVFLOW_RESULT")
   ...
   passed: !program.is_empty()
   ```

   It never validates `-z <prompt> --yolo --accept-hooks`, environment injection, health behavior, or delegation probing. A Hermes driver returning `("hermes", vec!["--interactive"])` with no environment would still pass the shared suite.

7. **The Hermes transport tests do not exercise Hermes arguments or environment**

   **File:** [phase7_cli.rs:1793-1804](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-cli/tests/phase7_cli.rs:1793)

   `hermes_stub()` branches only on `--version` and `tools list`; its launch body ignores all remaining arguments and never checks `HERMES_ACCEPT_HOOKS`.

   **Reproduction:** Remove `-z`, `--yolo`, `--accept-hooks`, or the environment assignment from the driver; the three transport tests can remain green.

8. **The hung-process test does not verify monitor cleanup**

   **File:** [phase7_cli.rs:1916-1930](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-cli/tests/phase7_cli.rs:1916)

   ```rust
   Command::new("kill").arg(pid.to_string()).status()
   ...
   let _reap = MonitorReapGuard::after_launch(&gated);
   ```

   The test manually kills Hermes before waiting for the gate. It does not verify monitor-initiated timeout cleanup, process-group cleanup, or descendant termination.

   **Reproduction:** Break monitor cleanup while retaining the manual `kill`; the test can still pass as long as the state eventually gates.
hook: Stop
hook: Stop Completed
tokens used
122,086
CODE REVIEW REPORT — REQUEST CHANGES

Architectural status: BLOCK

### Warning

1. **Antigravity bypasses the legacy opt-out**

   **File:** [preflight.rs:981-985](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-cli/src/preflight.rs:981)

   ```rust
   if stream_launch_enabled(state.agent, Stage::Code, state.legacy_claude_launch)
       && (state.agent == AgentKind::Claude || state.agent == AgentKind::Antigravity)
   {
       return ConditionState::Holds;
   }
   ```

   `stream_launch_enabled` ignores `legacy_claude_launch` for Antigravity, so an Antigravity state with the legacy opt-out set still returns `Holds`.

   **Reproduction:** `AgentKind::Antigravity`, `Mode::Auto`, `legacy_claude_launch = true` → unattended launch is permitted. The existing tests cover Claude with the opt-out, but not Antigravity with it.

2. **The auto-mode unlock has no runtime dogfood proof**

   **File:** [preflight.rs:981-985](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-cli/src/preflight.rs:981)

   The predicate permits a brand-new `State::new(..., AgentKind::Antigravity, Mode::Auto, ...)` without any persisted evidence that supervised dogfooding occurred. This contradicts the nearby comment at lines 974-980, which says Antigravity should remain refused until a real dogfood run.

   **Reproduction:** The existing `unattended_launch_shape_condition_antigravity_allowed` fixture constructs a fresh state and immediately gets `Holds`.

3. **Delegation parsing accepts disabled toolsets**

   **File:** [hermes.rs:97-101](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-core/src/agents/hermes.rs:97)

   ```rust
   let lower = line.to_ascii_lowercase();
   if lower.contains("delegation") && lower.contains("enabled") {
       return true;
   }
   ```

   This is substring co-occurrence, not status parsing.

   **Reproduction:**

   ```text
   ✗ disabled delegation (enabled terminal tools only)
   ```

   returns `true`, advertising subagent dispatch when delegation is disabled. The negative test only places `disabled` and `enabled` on separate lines.

4. **Hermes probes can hang indefinitely**

   **File:** [hermes.rs:58-60](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-core/src/agents/hermes.rs:58)

   ```rust
   Command::new("hermes").arg("--version").output()
   ```

   **File:** [hermes.rs:76-79](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-core/src/agents/hermes.rs:76)

   ```rust
   Command::new("hermes").args(["tools", "list"]).output()
   ```

   Both synchronously wait without a timeout.

   **Reproduction:** Put an executable named `hermes` earlier on `PATH` containing `sleep 1d`; doctor or capability discovery never returns.

5. **Graduation evidence is not reproducible**

   **Files:** [42-VERIFICATION.md:18-25](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/.planning/phases/42-hermes-driver/42-VERIFICATION.md:18), [42-UAT.md:18-54](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/.planning/phases/42-hermes-driver/42-UAT.md:18)

   The artifacts claim bounded quiet gaps and a held 60-minute timeout, but provide no timestamps, sample count, maximum gap, command transcript, or raw timeout evidence. The single `ConditionState::Holds` unit test cannot establish that supervised dogfooding happened or that the timeout margin is safe.

### Info

6. **The conformance suite does not test Hermes’ launch contract**

   **File:** [agents/mod.rs:146-169](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-core/src/agents/mod.rs:146)

   ```rust
   passed: prompt.contains("DEVFLOW_RESULT")
   ...
   passed: !program.is_empty()
   ```

   It never validates `-z <prompt> --yolo --accept-hooks`, environment injection, health behavior, or delegation probing. A Hermes driver returning `("hermes", vec!["--interactive"])` with no environment would still pass the shared suite.

7. **The Hermes transport tests do not exercise Hermes arguments or environment**

   **File:** [phase7_cli.rs:1793-1804](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-cli/tests/phase7_cli.rs:1793)

   `hermes_stub()` branches only on `--version` and `tools list`; its launch body ignores all remaining arguments and never checks `HERMES_ACCEPT_HOOKS`.

   **Reproduction:** Remove `-z`, `--yolo`, `--accept-hooks`, or the environment assignment from the driver; the three transport tests can remain green.

8. **The hung-process test does not verify monitor cleanup**

   **File:** [phase7_cli.rs:1916-1930](/var/home/denniyahh/Github/devflow/.worktrees/phase-42/crates/devflow-cli/tests/phase7_cli.rs:1916)

   ```rust
   Command::new("kill").arg(pid.to_string()).status()
   ...
   let _reap = MonitorReapGuard::after_launch(&gated);
   ```

   The test manually kills Hermes before waiting for the gate. It does not verify monitor-initiated timeout cleanup, process-group cleanup, or descendant termination.

   **Reproduction:** Break monitor cleanup while retaining the manual `kill`; the test can still pass as long as the state eventually gates.
