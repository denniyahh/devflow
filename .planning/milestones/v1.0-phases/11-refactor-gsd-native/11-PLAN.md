---
phase: 11-refactor-gsd-native
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/workflow.rs
  - crates/devflow-core/src/config.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/agents/claude.rs
  - crates/devflow-core/src/agents/codex.rs
  - crates/devflow-core/src/agents/opencode.rs
  - crates/devflow-core/src/agent.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/monitor.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/worktree.rs
  - crates/devflow-core/src/version.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-core/Cargo.toml
autonomous: true
requirements:
  - CORE-01
  - CORE-02
  - CORE-03
  - CLI-01
  - CLI-02
  - GATE-01
  - GATE-02
  - HOOK-01
  - VERSION-01

must_haves:
  truths:
    - "devflow start --phase 11 --agent claude --mode auto launches agent and auto-advances through Define→Plan→Code→Validate without blocking"
    - "devflow start --phase 11 --mode supervise writes Validate gate file and blocks until response.json appears"
    - "State machine only has 5 stages: Define, Plan, Code, Validate, Ship"
    - "No .devflow.yaml file is read — all behavior is CLI-flag-driven"
    - "Git flow branch names are hardcoded constants (main, develop, feature/)"
    - "cargo test passes with no test regressions"
  artifacts:
    - crates/devflow-core/src/stage.rs (new — Stage enum)
    - crates/devflow-core/src/mode.rs (new — Mode enum + gate decision)
    - crates/devflow-core/src/gates.rs (new — gate file protocol)
    - crates/devflow-core/src/hooks.rs (new — branch/docs/changelog/version hooks)
    - crates/devflow-core/src/prompt.rs (new — stage-specific agent prompts)
    - crates/devflow-cli/src/main.rs (rewritten CLI)
  key_links:
    - "Stage::next() → stage-specific prompt → agent adapter → child process"
    - "Monitor polls agent exit → agent_result → Validate decision tree → gate or auto-advance"
    - "Gate file → Hermes cron polls → Human response → DevFlow reads response.json → ack"
---

# Phase 11 Plan — Refactor to GSD-Native Architecture

**Target:** v2.0.0 | **Tests:** `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check`

## Execution Order

```
11a (Stage enum + State struct) → 11b (Mode system) → 11c (Gate file protocol)
→ 11d (Agent prompts rewrite) → 11e (Config simplification)
→ 11f (Hooks: branch/docs/changelog/version) → 11g (CLI rewrite)
→ 11h (Ship stage rewrite) → 11i (Remove old code) → 11j (Versioning)
→ 11k (Tests + docs + cleanup)
```

**Rationale:** 11a must come first — all subsequent areas depend on the new `Stage` enum
existing. 11b (mode) gates the decision tree in 11c (gates). 11d (prompts) needs the
stage definitions from 11a. 11e (config simplification) is decoupled but must precede
11g (CLI) since the CLI imports config types. 11f (hooks) and 11h (ship) are largely
independent of each other but both depend on 11a-11c. 11i removes dead code after
everything new is in place. 11j finalizes versioning. 11k wraps with tests and docs.

---

## 11a — Stage Enum + State Struct Rewrite

**Goal:** Replace the 9-step `Step` enum with a 5-stage `Stage` enum. Rewrite `State`
to remove skip logic and add mode/gate fields.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Create `crates/devflow-core/src/stage.rs` with `Stage` enum: `Define`, `Plan`, `Code`, `Validate`, `Ship`. Derive `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`. Serde `rename_all = "lowercase"`. | `crates/devflow-core/src/stage.rs` (new) | Enum compiles, `Stage::Define.to_string() == "define"` |
| 2 | Implement `Stage::next()`: Define→Plan→Code→Validate→Ship→None. Single linear chain, no branching. | `crates/devflow-core/src/stage.rs` | `Stage::Plan.next() == Some(Stage::Code)`, `Stage::Ship.next() == None` |
| 3 | Implement `Stage::is_gate()`: returns `true` for `Validate` and `Ship` (these fire gates to Hermes). `Stage::is_agent_stage()`: returns `true` for `Define`, `Plan`, `Code` (these launch agents). | `crates/devflow-core/src/stage.rs` | `Stage::Validate.is_gate() == true`, `Stage::Code.is_agent_stage() == true` |
| 4 | Implement `Stage::gsd_command()`: returns the GSD slash command string for each stage (e.g., `Code` → `/gsd-execute-phase {N}`). | `crates/devflow-core/src/stage.rs` | `Stage::Define.gsd_command() == "/gsd-discuss-phase {N}"` |
| 5 | Rewrite `State` struct in `state.rs`: replace `step: Step` with `stage: Stage`. Remove `agent_pid`, `monitor_pid`, `agent_label` (move to runtime context). Add `mode: Mode`, `gate_pending: bool`. Keep `phase`, `agent`, `started_at`, `project_root`, `worktree_path`. | `crates/devflow-core/src/state.rs` | Struct compiles, serde round-trips |
| 6 | Remove `Step` enum, `Step::next()`, `is_waiting()`, `is_skippable()`, `Display` impl for `Step`. Remove `State::advance()` and `State::advance_skipping()`. | `crates/devflow-core/src/state.rs` | `grep Step` in state.rs returns nothing (except import removal comments) |
| 7 | Add `State::new(phase, agent, mode, project_root)` constructor. | `crates/devflow-core/src/state.rs` | `State::new(1, Agent::Claude, Mode::Auto, ...)` creates valid state with `Stage::Define` |
| 8 | Update `lib.rs`: add `pub mod stage;`, remove old Step re-export, add `pub use stage::Stage;`. | `crates/devflow-core/src/lib.rs` | `cargo check -p devflow-core` passes |
| 9 | Update all imports throughout the crate that reference `Step` → point to `Stage`. Fix compile errors incrementally. | All `.rs` files in devflow-core | `cargo check -p devflow-core` passes |

**Estimated:** 2 commits, ~200 lines new, ~150 lines removed.

---

## 11b — Mode System

**Goal:** Add `Mode` enum (`Auto`, `Supervise`) and mode-driven gate decision logic.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Create `crates/devflow-core/src/mode.rs` with `Mode` enum: `Auto`, `Supervise`. Derive `Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize`. Implement `FromStr` for CLI parsing. | `crates/devflow-core/src/mode.rs` (new) | `"auto".parse::<Mode>() == Ok(Mode::Auto)` |
| 2 | Implement `Mode::should_gate(stage: Stage) -> bool`: Auto mode gates only at Ship (and after 3 consecutive Validate failures). Supervise mode gates at every Validate. | `crates/devflow-core/src/mode.rs` | `Mode::Auto.should_gate(Stage::Validate) == false`, `Mode::Supervise.should_gate(Stage::Validate) == true` |
| 3 | Implement `Mode::should_auto_loop(stage: Stage) -> bool`: Auto mode auto-loops Code↔Validate. Supervise mode requires human approval at Validate. | `crates/devflow-core/src/mode.rs` | `Mode::Auto.should_auto_loop(Stage::Validate) == true` |
| 4 | Add `consecutive_failures: u32` field to runtime context (not persisted state) — tracks Validate→Code loops. When ≥3 in Auto mode, force a gate. | `crates/devflow-core/src/mode.rs` | After 3 Validate failures in Auto, `should_gate` returns `true` |
| 5 | Update `lib.rs`: add `pub mod mode;`, `pub use mode::Mode;`. | `crates/devflow-core/src/lib.rs` | `cargo check` passes |

**Estimated:** 1 commit, ~80 lines new.

---

## 11c — Gate File Protocol

**Goal:** Formalize the spike-tested gate file protocol into a production module.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Create `crates/devflow-core/src/gates.rs` with gate file schema structs: `GateFile { phase, stage, context, timestamp }`, `GateResponse { approved, note, responded_by }`, `GateAck { received: true }`. All derive Serialize/Deserialize. | `crates/devflow-core/src/gates.rs` (new) | Structs round-trip through serde_json |
| 2 | Implement `Gates::write_gate(phase, stage, context)`: writes `NN-{stage}.json` to `.devflow/gates/`. Creates directory if missing. | `crates/devflow-core/src/gates.rs` | File exists at `.devflow/gates/11-validate.json` with correct content |
| 3 | Implement `Gates::poll_response(phase, stage, timeout_secs)`: polls `.devflow/gates/NN-{stage}.response.json` with exponential backoff (1s→2s→4s...max 60s). Returns `Option<GateResponse>`. | `crates/devflow-core/src/gates.rs` | Returns `Some(response)` when file appears, `None` on timeout |
| 4 | Implement `Gates::ack(phase, stage)`: writes `.devflow/gates/NN-{stage}.ack.json` to signal receipt. Hermes cron can then clean up. | `crates/devflow-core/src/gates.rs` | Ack file created with `{"received": true}` |
| 5 | Implement `Gates::cleanup(phase, stage)`: removes gate + response + ack files for a completed stage. | `crates/devflow-core/src/gates.rs` | Files removed from `.devflow/gates/` |
| 6 | Add `GateAction` enum: `Advance`, `LoopBack(Stage)`, `Abort(String)`. Parsed from `GateResponse.approved` + note content. | `crates/devflow-core/src/gates.rs` | `response.approved=true` → `Advance`, `approved=false, note="fix"` → `LoopBack(Code)` |
| 7 | Update `lib.rs`: add `pub mod gates;`. | `crates/devflow-core/src/lib.rs` | `cargo check` passes |

**Estimated:** 1 commit, ~180 lines new.

---

## 11d — Agent Prompts Rewrite

**Goal:** Replace the 68-line `phase_prompt()` with stage-specific prompts driven by GSD slash commands.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Create `crates/devflow-core/src/prompt.rs` with `stage_prompt(stage, phase) -> String`. Each stage returns a minimal prompt: (a) the GSD slash command from `Stage::gsd_command()`, (b) the DEVFLOW_RESULT protocol marker, (c) stage-specific context (e.g., Code stage adds `--gaps-only` on fix loops). | `crates/devflow-core/src/prompt.rs` (new) | `stage_prompt(Stage::Code, 11)` contains `/gsd-execute-phase 11` and `DEVFLOW_RESULT` |
| 2 | Implement `fix_prompt(fix_type, phase) -> String` for audit-fix and gaps-only variants used in Code→Validate loops. | `crates/devflow-core/src/prompt.rs` | `fix_prompt(FixType::AuditFix, 11)` contains `/gsd-audit-fix` |
| 3 | Remove `phase_prompt()` from `agents/mod.rs`. Delete the 68-line prompt template. | `crates/devflow-core/src/agents/mod.rs` | `grep "phase_prompt" agents/mod.rs` returns nothing |
| 4 | Update agent adapters (`claude.rs`, `codex.rs`, `opencode.rs`): change `exec_command(phase)` to `exec_command(phase, prompt_text: &str)`. The adapter formats the prompt text into its CLI flags (Claude: `-p`, Codex: `exec`, OpenCode: direct arg). | `crates/devflow-core/src/agents/claude.rs`, `codex.rs`, `opencode.rs` | Each adapter's exec_command accepts a prompt string and wraps it correctly |
| 5 | Update the `Agent` trait signature: `fn exec_command(&self, phase: u32, prompt: &str) -> (&'static str, Vec<String>)`. | `crates/devflow-core/src/agents/mod.rs` | Trait compiles with new signature |
| 6 | Update `lib.rs`: add `pub mod prompt;`. | `crates/devflow-core/src/lib.rs` | `cargo check` passes |
| 7 | Update tests in `agents/mod.rs` tests module for new prompt signatures. | `crates/devflow-core/src/agents/mod.rs` | `cargo test -p devflow-core` agent tests pass |

**Estimated:** 1 commit, ~120 lines new, ~70 lines removed.

---

## 11e — Config Simplification

**Goal:** Strip `.devflow.yaml` parsing, auto_* toggles, `should_skip()`. Keep only runtime config types needed internally. Remove `devflow init` and `devflow config` CLI commands.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Delete `AutomationConfig` struct and all its fields (`auto_branch`, `auto_verify`, `auto_docs`, `auto_version`, `auto_ship`, `auto_cleanup`, `auto_plan`, `verify_command`, `lint_command`, `docs_command`, `continue_on_error`, `docs_auto_commit`). | `crates/devflow-core/src/config.rs` | Struct gone, `grep "AutomationConfig" config.rs` returns nothing |
| 2 | Delete `VersionConfig` struct (versioning is now hybrid Git-based SemVer, auto-detected). Keep only `VersionConfig::auto_detect()` logic in `version.rs`. | `crates/devflow-core/src/config.rs` | `grep "VersionConfig" config.rs` returns nothing |
| 3 | Simplify `Config` struct to a marker/empty struct or delete entirely. The module becomes a thin runtime-values holder with `GitFlowConfig` only. | `crates/devflow-core/src/config.rs` | `Config` is minimal — just holds runtime git-flow constants |
| 4 | Delete `Config::load()`, `parse_config()`, `should_skip()`, `to_yaml()`, `ConfigError`, `clean_value()`, `parse_bool()`. | `crates/devflow-core/src/config.rs` | `grep "fn load\|fn parse_config\|fn should_skip\|fn to_yaml\|ConfigError\|fn clean_value\|fn parse_bool" config.rs` returns nothing |
| 5 | Delete config-related tests (`parses_devflow_yaml_shape`, `defaults_match_documented_schema`, etc.) — or rewrite to test new struct shape. | `crates/devflow-core/src/config.rs` | `cargo test -p devflow-core` passes with remaining tests |
| 6 | Hardcode git flow constants in `git.rs`: `MAIN = "main"`, `DEVELOP = "develop"`, `FEATURE_PREFIX = "feature/"`. Remove `GitFlowConfig` struct usage from `git.rs`. | `crates/devflow-core/src/git.rs` | `GitFlow::new()` takes no config arg (uses constants) |
| 7 | Remove `devflow init` and `devflow config` CLI commands from `main.rs`. | `crates/devflow-cli/src/main.rs` | `grep "Command::Init\|Command::Config" main.rs` returns nothing |
| 8 | Remove associated handler functions (`init()`, `show_config()`) from `main.rs`. | `crates/devflow-cli/src/main.rs` | Functions deleted |

**Estimated:** 1 commit, ~400 lines removed, ~30 lines changed.

---

## 11f — Hooks Module

**Goal:** Extract branching, docs, changelog, and version bump into a hooks module that fires at stage transitions, not as workflow steps.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Create `crates/devflow-core/src/hooks.rs` with `Hook` enum: `BranchCreate`, `BranchCleanup`, `DocsUpdate`, `ChangelogAppend`, `VersionBump`. | `crates/devflow-core/src/hooks.rs` (new) | Enum compiles |
| 2 | Implement `Hook::run(&self, context: &HookContext)` — dispatches to the appropriate hook function. `HookContext` carries `phase`, `project_root`, `stage`, `git_flow`. | `crates/devflow-core/src/hooks.rs` | Each hook variant runs its implementation |
| 3 | Implement `BranchCreate` hook: creates `feature/phase-NN` branch from develop using hardcoded git-flow constants. Moved from `start()` in CLI. | `crates/devflow-core/src/hooks.rs` | Branch created, `git branch` shows `feature/phase-11` |
| 4 | Implement `BranchCleanup` hook: deletes merged feature branch after Ship completes. Moved from old Cleaning stage. | `crates/devflow-core/src/hooks.rs` | Branch deleted after merge |
| 5 | Implement `DocsUpdate` hook: runs `cargo doc --no-deps` and commits docs changes. Moved from old Docsing stage. | `crates/devflow-core/src/hooks.rs` | Docs generated, commit created |
| 6 | Implement `ChangelogAppend` hook: appends phase summary to `CHANGELOG.md`. | `crates/devflow-core/src/hooks.rs` | `CHANGELOG.md` updated with new entry |
| 7 | Implement `VersionBump` hook: auto-detects version file, computes next version via Hybrid Git-Based SemVer, bumps it. Moved from old Shipping stage's version step. | `crates/devflow-core/src/hooks.rs` | Version file updated, `git tag` shows new tag |
| 8 | Define `HookTrigger` map: which hooks fire at which stage transitions. Define→Plan: none. Plan→Code: none. Code→Validate: none. Validate→Ship: DocsUpdate, ChangelogAppend. Ship→done: VersionBump, BranchCleanup. | `crates/devflow-core/src/hooks.rs` | `hooks_for_transition(Stage::Validate, Stage::Ship)` returns `[DocsUpdate, ChangelogAppend]` |
| 9 | Update `lib.rs`: add `pub mod hooks;`. | `crates/devflow-core/src/lib.rs` | `cargo check` passes |

**Estimated:** 2 commits, ~250 lines new.

---

## 11g — CLI Rewrite

**Goal:** Redesign the CLI for v2.0.0: new `--mode` flag, `--dry-run`, `devflow test`, remove dead commands.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Redesign `Command::Start`: add `--mode auto|supervise` (required), `--dry-run` (optional, prints what would happen without launching agents). Keep `--phase`, `--agent`, `--worktree`, `--force`, `--project`. Remove `--monitor` (always on). | `crates/devflow-cli/src/main.rs` | `devflow start --phase 11 --agent claude --mode auto --dry-run` prints plan, exits 0 |
| 2 | Rewrite `start()` handler: creates state with new `Stage`, `Mode`. Launches agent via monitor (always monitor mode — no blocking path). Monitor polls, advances through stages, fires gates per mode. | `crates/devflow-cli/src/main.rs` | Agent launched, monitor running, auto-advance works |
| 3 | Implement `--dry-run` mode: prints stage sequence, agent commands, gate points, hooks that would fire. Exits without spawning anything. | `crates/devflow-cli/src/main.rs` | `--dry-run` output shows full pipeline |
| 4 | Add `devflow test` command: runs `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check`. Local-only quality checks for agent use. | `crates/devflow-cli/src/main.rs` | `devflow test` runs all checks, exits with combined status |
| 5 | Remove `devflow check` (replaced by monitor auto-advance + gates). Remove `devflow verify`, `devflow lint`, `devflow docs` (replaced by hooks + validate-phase). Remove `devflow ship`, `devflow confirm`, `devflow rejectpr` (replaced by GSD ship + gate). | `crates/devflow-cli/src/main.rs` | `grep "Command::Check\|Command::Verify\|Command::Lint\|Command::Docs\|Command::Ship\|Command::Confirm\|Command::Rejectpr" main.rs` returns nothing |
| 6 | Keep: `start`, `parallel`, `sequentagent`, `reference`, `cleanup`, `status`, `list`, `recover`, `doctor`, `test` (new). | `crates/devflow-cli/src/main.rs` | `cargo build` produces binary with these subcommands |
| 7 | Update `status` command: shows current `Stage` instead of `Step`, shows `Mode`, shows gate status (pending/acknowledged). | `crates/devflow-cli/src/main.rs` | `devflow status` shows "Stage: Code | Mode: Auto | Gate: none" |
| 8 | Remove all unused handler functions (check, verify, lint, docs, ship, confirm, rejectpr, init, config handlers). | `crates/devflow-cli/src/main.rs` | `cargo check -p devflow-cli` passes with no dead code warnings |

**Estimated:** 2 commits, ~500 lines changed, ~600 lines removed.

---

## 11h — Ship Stage Rewrite

**Goal:** Replace old ship.rs with GSD-native ship that delegates to `/gsd-ship` + `/gsd-code-review` gate.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Rewrite `ship.rs`: new `ship_phase()` function that launches agent with `/gsd-ship {N}` prompt, waits for completion, then launches `/gsd-code-review`. If review passes, runs VersionBump + BranchCleanup hooks. If review fails, loops back to Code stage. | `crates/devflow-core/src/ship.rs` | Ship stage completes with version bump and merged PR |
| 2 | Remove old ship logic: `gh pr create` calls, manual version bump, confirm/reject flow. All replaced by GSD commands + hooks. | `crates/devflow-core/src/ship.rs` | Old ship functions deleted |
| 3 | Gate Ship via code-review: after `/gsd-ship {N}` completes, the monitor reads review result. If issues found, the decision tree loops back to Code→Validate→Ship. | `crates/devflow-core/src/ship.rs` | Failed review → agent receives Code prompt |
| 4 | Update `ShipError` error type: remove old variants, add `ReviewFailed(String)`, `AgentFailed(String)`. | `crates/devflow-core/src/ship.rs` | Error types compile |

**Estimated:** 1 commit, ~150 lines new, ~200 lines removed.

---

## 11i — Remove Dead Code

**Goal:** Delete all modules and functions declared as "throw away" in CONTEXT.md.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Delete `crates/devflow-core/src/verify.rs` entirely. Verify/lint/docs are now GSD validate-phase + hooks. | `crates/devflow-core/src/verify.rs` | File deleted, `cargo check` still passes |
| 2 | Remove `pub mod verify;` from `lib.rs`. Remove `pub use verify::*` if any. | `crates/devflow-core/src/lib.rs` | No verify references |
| 3 | Remove `Step`-related dead code: if any `Step` imports or references remain after 11a, clean them up. | Various `.rs` files | `grep -r "Step" crates/` returns only new `Stage`-related matches |
| 4 | Remove `should_skip` references: any remaining calls to config skip logic. | Various `.rs` files | `grep -r "should_skip\|advance_skipping" crates/` returns nothing |
| 5 | Remove blocking agent launch path from `agent.rs`: delete `capture_agent_output()` public function (monitor handles capture now). Keep `launch_agent()` and `agent_running()`. | `crates/devflow-core/src/agent.rs` | `cargo check` passes |
| 6 | Remove `continue_on_error` references from all modules. | Various `.rs` files | `grep -r "continue_on_error" crates/` returns nothing |
| 7 | Delete `devflow check` handler function from CLI. | `crates/devflow-cli/src/main.rs` | Function gone |

**Estimated:** 1 commit, ~500 lines removed.

---

## 11j — Hybrid Git-Based SemVer

**Goal:** Implement the new versioning scheme: MAJOR from version file, MINOR from git tag count, PATCH from commit count since last minor tag.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Rewrite `version.rs`: add `compute_version(project_root) -> Result<Version>`. Reads MAJOR from auto-detected version file (Cargo.toml `workspace.package.version`), counts git tags for MINOR, counts commits since last minor tag for PATCH. | `crates/devflow-core/src/version.rs` | Returns correct `Version { major: 2, minor: 0, patch: 0 }` for initial v2.0.0 |
| 2 | Implement `detect_version_file(project_root) -> Option<PathBuf>`: checks Cargo.toml workspace, then pyproject.toml, then package.json. Returns first found. | `crates/devflow-core/src/version.rs` | `detect_version_file("/home/denniyahh/Github/devflow")` returns `Some("Cargo.toml")` |
| 3 | Implement `read_major_version(path) -> Result<u32>`: parses the version file to extract the major component. | `crates/devflow-core/src/version.rs` | Reads "0.5.0" from Cargo.toml → returns 0 (will become 2 via manual bump in v2.0.0) |
| 4 | Implement `count_git_tags(project_root) -> Result<u32>`: runs `git tag | wc -l` to count all tags for MINOR. | `crates/devflow-core/src/version.rs` | Returns current tag count |
| 5 | Implement `commits_since_last_minor_tag(project_root) -> Result<u32>`: runs `git rev-list --count <last-tag>..HEAD` for PATCH. | `crates/devflow-core/src/version.rs` | Returns commit count |
| 6 | Update the `VersionBump` hook (from 11f) to use `compute_version()` instead of old version logic. | `crates/devflow-core/src/hooks.rs` | Hook bumps to correct version |
| 7 | Remove old versioning logic: `VersionConfig`, scheme selection, calver support, build_number. | `crates/devflow-core/src/version.rs` | Old code gone |

**Estimated:** 1 commit, ~200 lines new, ~100 lines removed.

---

## 11k — Tests, Docs, and Final Cleanup

**Goal:** Ensure all tests pass, add new tests for the v2.0.0 architecture, update documentation.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Rewrite `state.rs` tests: all old `Step` tests replaced with `Stage` tests. Test `Stage::next()` chain, `is_gate()`, `is_agent_stage()`, `gsd_command()`. Test `State::new()` with Mode. | `crates/devflow-core/src/state.rs` | `cargo test -p devflow-core -- state` passes |
| 2 | Add tests for `stage.rs`: serde round-trip, display output, GSD command correctness for all stages. | `crates/devflow-core/src/stage.rs` | All stage tests pass |
| 3 | Add tests for `mode.rs`: `FromStr` parsing, `should_gate()` for all stage×mode combinations, auto-loop logic, 3-failure gate trigger. | `crates/devflow-core/src/mode.rs` | All mode tests pass |
| 4 | Add tests for `gates.rs`: write gate file, poll response (with temp dir), ack, cleanup, `GateAction` parsing. | `crates/devflow-core/src/gates.rs` | All gate tests pass |
| 5 | Add tests for `prompt.rs`: verify each stage prompt contains correct GSD slash command and DEVFLOW_RESULT marker. Verify fix prompts. | `crates/devflow-core/src/prompt.rs` | All prompt tests pass |
| 6 | Add tests for `hooks.rs`: each hook runs without error in temp git repo. Verify branch names, version bumps, changelog entries. | `crates/devflow-core/src/hooks.rs` | All hook tests pass |
| 7 | Update `workflow.rs` tests: `advance_state()` tests removed (now monitor-driven). Keep `save_state`/`load_state`/`clear_state` tests, update for new `State` fields. | `crates/devflow-core/src/workflow.rs` | Persistence tests pass |
| 8 | Update `agents/mod.rs` tests: update `exec_command` calls to pass prompt string. Update `claude_and_codex_share_identical_prompt_text` to use `stage_prompt()`. | `crates/devflow-core/src/agents/mod.rs` | Agent tests pass |
| 9 | Update `git.rs` tests: update `GitFlow::new()` calls to not pass config. | `crates/devflow-core/src/git.rs` | Git tests pass |
| 10 | Update `version.rs` tests: test `compute_version()` in a temp git repo with known tags and commits. | `crates/devflow-core/src/version.rs` | Version tests pass |
| 11 | Run full test suite: `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check`. Fix all failures. | Project root | All green |
| 12 | Update `AGENTS.md` and `crates/devflow-core/src/lib.rs` doc comments to reflect v2.0.0 architecture. | `AGENTS.md`, `lib.rs` | Docs accurate |
| 13 | Delete `.devflow.yaml` from project root (no longer needed). | Project root | File deleted, `devflow start` still works |
| 14 | Delete any remaining `Step`-related dead imports, comments, or references found by `grep -r "Step\b" crates/ --include="*.rs"`. | Various | Only `Stage` references remain |

**Estimated:** 2 commits, ~400 lines new tests, ~200 lines docs.

---

## Dependency Graph

```
11a (Stage+State)
├── 11b (Mode) ──────┐
├── 11d (Prompts) ───┤
├── 11e (Config) ────┤
└── 11f (Hooks) ─────┤
                     ├── 11g (CLI) ──┐
                     └── 11h (Ship) ─┤
                                      ├── 11i (Remove dead)
                                      ├── 11j (Versioning)
                                      └── 11k (Tests+docs)
```

## Risk Mitigation

| Risk | Impact | Mitigation |
|---|---|---|
| Agent prompt breakage | Agents don't understand new prompt format | Test with `--dry-run` first; keep DEVFLOW_RESULT protocol unchanged |
| Gate file race conditions | Hermes and DevFlow step on each other | Atomic file writes (write-to-temp + rename); single-polling loop |
| Monitor regression | Auto-advance stops working | Keep existing monitor PID loop; only change the advance logic, not the polling |
| Git flow hardcoding | User has non-standard branch names | Document as opinionated; custom setups can fork or use Phase 12 config |
| Version computation errors | Wrong version shipped | Extensive unit tests in temp git repos; `--dry-run` shows computed version |
