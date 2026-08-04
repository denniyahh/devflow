# Codex Review — Phase 9 OSS Polish Requirements

## Summary

Phase 9 has the right product goal, but the current requirements mix three different release shapes:

1. OSS contributor polish: docs, CI badges, devcontainer, removal of local-only assumptions.
2. Requirements correction: align docs/planning with what the code actually supports.
3. A new audit-log subsystem: core data model, CLI query surface, lifecycle instrumentation, rotation, tests, and cleanup/doctor integration.

That is too broad for one phase if "phase" means one reviewable implementation unit. I recommend making Phase 9 the OSS/docs/infrastructure cleanup and deferring the audit log to a dedicated Phase 10.

## Findings

### HIGH — `.planning/` requirement is factually wrong for the current repo

Phase 9 says `.planning/` is "Gitignored, GSD-specific" and should stay gitignored (`CONTEXT.md:19`). `.gitignore` does ignore `.planning/` (`.gitignore:22-26`), but existing `.planning` files are already tracked, including `.planning/PROJECT.md`, `.planning/ROADMAP.md`, `.planning/codebase/*`, and earlier phase files (`git ls-files .planning` confirms this).

This matters because the phase says "document that `.planning/` is a GSD convention, not a devflow requirement," while the runtime prompt currently requires agents to read `.planning/ROADMAP.md` and `.planning/phases/{phase}/CONTEXT.md` (`crates/devflow-core/src/agents/mod.rs:32-36`). Shipping this phase as written would preserve a contradiction: `.planning` is "not a devflow requirement" for contributors, but it is a first-class input to the phase execution prompt.

Recommendation: choose one model before implementation:

- If `.planning` is internal dogfooding only, remove hard-coded `.planning` assumptions from `phase_prompt()` or make planning paths configurable.
- If `.planning` remains part of DevFlow's workflow contract, stop describing it as merely GSD-specific and document the expected layout.

### HIGH — Agent-agnosticism success criterion is stale

Phase 9 says adding a new agent should require exactly one new adapter file, one `AgentKind` variant, and one `adapter_for` entry (`CONTEXT.md:39`). Current code already requires more coordination than that:

- `Agent` parsing/display/error text must be updated in `state.rs` (`crates/devflow-core/src/state.rs:110-177`).
- `adapter_for()` must be updated (`crates/devflow-core/src/agents/mod.rs:72-79`).
- The new adapter module must be declared/exported (`crates/devflow-core/src/agents/mod.rs:82-90`).
- Tests and docs need updates.

There is also an existing conflict around OMX: README advertises `--agent omx` (`README.md:82-89`), an `omx.rs` adapter exists and is exported (`crates/devflow-core/src/agents/mod.rs:84,89`), but `Agent::Omx` and parser support are disabled (`crates/devflow-core/src/state.rs:113,152,166`). A new contributor following README will hit an unsupported-agent error.

Recommendation: update the Phase 9 requirement from "3 changes max" to "document the adapter checklist and remove stale disabled paths," or explicitly include a registry/configuration refactor if "3 changes max" is truly required.

### HIGH — Audit log is a full feature, not polish

The audit log section requires JSONL storage, schema stability, rotation, CLI filters, lifecycle instrumentation across branch/state/monitor/worktree/ship paths, `doctor` integration, and `cleanup` rotation/truncation (`CONTEXT.md:103-144`). Current CLI has no `audit` or `doctor` command (`crates/devflow-cli/src/main.rs:21-189`), and `cleanup` currently means worktree/branch cleanup (`crates/devflow-cli/src/main.rs:90-98,258`), so adding audit cleanup behavior would overload an existing command.

The monitor also runs as a detached shell script that redirects stdout and executes several `devflow check` calls (`crates/devflow-core/src/monitor.rs:92-102`). Instrumenting this correctly is non-trivial: if the monitor writes audit events from shell, schema and fail-soft behavior need separate handling; if it delegates to `devflow check`, some events such as `agent.launched` and `agent.exited` need new CLI/core hooks or helper commands.

Recommendation: defer audit logging to a separate phase with its own design contract. For Phase 9, keep only the documentation need: "document existing `.devflow/phase-NN-stdout`, exit, PID, state, and last-ship files as current diagnostics."

### MEDIUM — Scope is not realistic for one phase

One phase currently includes:

- Deleting or ignoring `distrobox.ini`.
- Architecture documentation.
- Agent-agnosticism verification/remediation.
- Devcontainer.
- Optional Dockerfile.
- CI fork safety, status badge, and release workflow.
- Full audit-log subsystem.

The first five are small-to-medium and reviewable together. The release workflow and audit subsystem both introduce external/platform risk and should each be isolated. The release workflow especially needs OS matrix design, artifact naming, signing/checksums policy, and GitHub token permissions; those requirements are not specified.

Recommendation: split:

- Phase 9: OSS polish: docs, README correction, CONTRIBUTING, devcontainer, `.gitignore`/`distrobox`, CI badge.
- Phase 10: audit log.
- Phase 11 or release task: binary release workflow.

### MEDIUM — External service and environment assumptions are under-specified

The CI requirement says PRs from forks must run with no secrets (`CONTEXT.md:95-99`). The current CI uses `pull_request` and only runs cargo commands (`.github/workflows/ci.yml:3-40`), which is a good base. But the release workflow requirement introduces GitHub Releases/artifact publishing without saying whether it runs on tags, manual dispatch, or release branches, and without permissions or token policy.

The devcontainer requirement installs `gh` (`CONTEXT.md:53-57`) but does not say whether authenticated `gh` is optional. That is fine for local contributors, but docs should not imply `gh auth login` is required for build/test. Dockerfile is marked optional, but success criteria do not state whether optional means excluded from phase completion.

Recommendation: add explicit acceptance criteria:

- PR CI must not use secrets or `pull_request_target`.
- Release workflow is out of scope unless a trigger, permissions block, artifact matrix, and no-secret fork behavior are specified.
- `gh` in devcontainer is convenience-only; build/test must work unauthenticated.
- Dockerfile is either deferred or not part of success criteria.

### MEDIUM — Public docs already conflict with current implementation

README still says agents are launched "in tmux" (`README.md:15`) and lists tmux as a requirement (`README.md:117-121`), but the monitor docs say it uses a detached child process and no scheduler/agent cooperation (`crates/devflow-core/src/monitor.rs:1-12`). README lists `devflow finish` (`README.md:100-101`), while current CLI exposes `confirm` and `rejectpr` (`crates/devflow-cli/src/main.rs:129-146,267-272`). README also advertises `omx`, which is disabled as noted above.

Recommendation: include README/CONTRIBUTING correctness as explicit Phase 9 requirements, not just architecture docs. This is more urgent for OSS readiness than a Dockerfile.

### LOW — Rust/toolchain requirements are not pinned coherently

README says Rust 1.91+ (`README.md:6,119`), CI uses `dtolnay/rust-toolchain@stable` (`.github/workflows/ci.yml:19,27,37`), and the proposed Dockerfile uses `rust:1.91-slim` (`CONTEXT.md:75`). There is no `rust-toolchain.toml` in the repo.

Recommendation: either add `rust-toolchain.toml` and use it consistently in CI/devcontainer/Docker, or document "stable Rust" and remove exact 1.91 claims.

### LOW — Configuration docs should cover the real schema and unsupported sample fields

Phase 9 asks for `.devflow.yaml` schema docs (`CONTEXT.md:30`). Current `Config` has `version`, `automation`, and `git_flow` fields (`crates/devflow-core/src/config.rs:9-111`). README's sample includes `git_flow.enabled` (`README.md:75-79`), but the actual parser schema shown in `GitFlowConfig` has no `enabled` field.

Recommendation: make schema documentation generated or verified against `Config`, and correct README examples during Phase 9.

## Missing Requirements

- Define whether `.planning/` is part of DevFlow's public workflow contract or only this repo's dogfooding convention.
- Decide the OMX outcome: remove it from README and exported modules, or re-enable it fully.
- Add README command-table correction to Phase 9 acceptance criteria.
- Add CONTRIBUTING instructions for fork PRs, local checks, and no required agent credentials for ordinary code contributions.
- Add a tracked toolchain policy (`stable` vs pinned version).
- Specify release workflow trigger, permissions, artifact names, checksums, and whether publishing GitHub Releases is in scope.
- For audit logging, define privacy/redaction policy before logging raw commands, paths, branch names, and PR URLs.
- For audit logging, define test fixtures and whether audit logs are enabled by default in tests.

## What To Defer Or Remove

Defer:

- Full audit log implementation (`AuditLog`, `devflow audit`, rotation, lifecycle instrumentation, `doctor`, cleanup integration).
- Release workflow for multi-platform binaries.
- Dockerfile unless it is actually used by CI or a documented contributor path.
- Hermes integration with audit tailing.
- Configurable log rotation.

Remove or revise:

- "Adding a new agent is 3 changes max" unless a registry refactor is in scope.
- "`doctor` checks audit log" until a `doctor` command exists.
- "`cleanup` offers to truncate/rotate old audit logs" unless `cleanup` is redesigned with subcommands or a separate `audit prune` command.
- "`.planning/` is gitignored" as a factual claim; it is ignored for new files but existing planning docs are tracked.

## Recommended Phase 9 Acceptance Criteria

1. README and CONTRIBUTING match the current CLI and supported agents.
2. `ARCHITECTURE.md` documents the actual crate boundaries, state machine, monitor model, worktree model, config schema, and adapter checklist.
3. `.planning` status is resolved and documented consistently.
4. `distrobox.ini` is removed from the tracked repo or explicitly documented as optional and ignored going forward.
5. Devcontainer builds with `cargo build` and supports `cargo test`, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
6. CI has a status badge, runs cargo test/fmt/clippy for PRs without secrets, and documents fork-safe behavior.

That is a realistic OSS-polish phase. The audit log is valuable, but it deserves a dedicated implementation phase.
