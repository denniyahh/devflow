# Architecture

DevFlow is an agent-agnostic CLI that automates a Git-flow development workflow:
branch → run a coding agent → verify → docs → ship → cleanup. This document
describes the implementation as it actually exists; treat the named source files
as the source of truth if this drifts.

## Crates

DevFlow is a two-crate Cargo workspace (`Cargo.toml`):

| Crate | Kind | Responsibility |
|---|---|---|
| `devflow-core` | Library (`crates/devflow-core`) | State machine, config, git/ship orchestration, versioning, agent adapters, monitor daemon, worktrees, recovery. Returns structured data; no terminal formatting. |
| `devflow-cli` | Binary (`crates/devflow-cli`) | Clap command parser and the user-facing `devflow` binary. Owns all printing; delegates logic to `devflow-core`. |

The split keeps workflow logic testable and independent of both the CLI surface
and any individual coding agent.

## State machine

The workflow is a deterministic step sequence (`crates/devflow-core/src/state.rs`,
persisted via `crates/devflow-core/src/workflow.rs`):

```
Idle → Branching → Planning → Executing → Verifying → Docsing → Shipping → Cleaning → Idle
```

- `Step::next()` defines the transitions; `Cleaning` is terminal (returns to `Idle`).
- `Step::is_waiting()` is true for `Planning` and `Executing` — the steps that block
  on user review (Planning) or agent completion (Executing).
- `Step::is_skippable()` is true for `Planning`, `Verifying`, `Docsing`, and `Shipping`.
  `State::advance_skipping()` consults `Config::should_skip()` so disabled
  automation steps are stepped over. The `auto_plan` config toggle controls whether
  `Planning` is auto-skipped (note: `Shipping` is never auto-skipped by
  config — see `should_skip`).
- `State` (step, phase, agent, PIDs, label, project root, optional worktree path)
  is serialized to `.devflow/state.json`.

## Agent model

Agent backends are isolated behind a trait (`crates/devflow-core/src/agents/mod.rs`):

- `Agent` trait: `name()`, `exec_command(phase) -> (program, args)`,
  `completion_signal_detected(output)`. Each adapter only knows how to launch
  its CLI headless.
- Supported `AgentKind`s (`state.rs`): `Claude`, `Codex`, `OpenCode`.
  Accepted names: `claude`, `codex`, `opencode` / `open-code`.
- `adapter_for(kind)` returns the boxed adapter for a kind.
- `phase_prompt(phase)` is the single shared instruction text handed to every
  agent — they differ only in CLI flags, not in prompt content (enforced by the
  `claude_and_codex_share_identical_prompt_text` test). The prompt directs the
  agent to read `CLAUDE.md`, `.planning/ROADMAP.md`,
  `.planning/phases/NN-*/CONTEXT.md`, and `AGENTS.md`, then implement, test,
  lint, format, commit per sub-task, and emit a `DEVFLOW_RESULT` marker.

## Completion evaluation

When an agent exits, DevFlow decides success/failure with three layers
(`crates/devflow-core/src/agent_result.rs`), tried in order:

1. **Layer 1 — `DEVFLOW_RESULT` marker (authoritative).** Scans the tail of the
   agent's captured stdout for the last `DEVFLOW_RESULT: {"status": ...}` line.
   If present, that verdict wins.
2. **Layer 2 — exit code + commit count (reliable fallback).** Reads the agent
   exit code from `.devflow/phase-NN-exit` and counts commits on
   `feature/phase-NN` (`develop..feature/phase-NN`). `exit == 0 && commits > 0`
   → success; `exit == 0 && commits == 0` → halt ("no work done");
   `exit != 0` → halt ("agent failed"). Unknown exit code falls through.
3. **Layer 3 — process gone + commits exist (last resort).** If neither marker
   nor exit code is available, the presence of commits is used as a heuristic
   and a warning is surfaced.

## Monitor daemon

`crates/devflow-core/src/monitor.rs` is the core automation primitive — no cron,
no scheduler, no agent cooperation. `devflow start --monitor` spawns a **detached
child process that owns the agent**:

1. Launches the agent (`program` + `args`), redirecting stdout to the phase
   stdout capture file and recording the agent PID.
2. Waits for the agent to exit and records its exit code to
   `.devflow/phase-NN-exit`.
3. Runs `devflow check` to advance the state machine through its remaining steps.

Because the monitor outlives the `devflow start` invocation, agent stdout keeps
flowing into the capture file and the exit code is still reaped after the CLI
process returns.

## Worktree model

`crates/devflow-core/src/worktree.rs` wraps plain `git worktree` commands. Each
agent can get an isolated working directory that shares the main repo's object
database, placed under `<project_root>/.worktrees/`. State and capture files
always live under the main `project_root`; only the agent's working directory
moves. This backs three CLI commands:

- `start --worktree` — run a single phase in its own worktree.
- `parallel` — run multiple phases concurrently, each in its own worktree.
- `sequentagent` — run phases in sequence, rebasing each onto the previous.
- `reference` — create or refresh a static snapshot worktree at
  `.worktrees/reference/`.

Branch integration uses `ensure_branch()` (create at a start point without
checking out) and `fast_forward_branch()` (move a ref forward only if it is a
descendant — refuses non-fast-forward updates).

## Git and ship model

`crates/devflow-core/src/git.rs` implements the Git-flow orchestration:

- `feature_start(phase)` / `feature_start_force(phase)` — create
  `feature/phase-NN` from `develop`.
- `feature_finish(phase)` — merge the feature branch into `develop` (`--no-ff`)
  and delete it.
- `release_start(version)` — **create/reset `release/{version}` from the current
  `HEAD`**, not from `develop`. `devflow ship` writes the version bump into the
  working tree first, so cutting from `HEAD` keeps commits unique to the shipped
  branch in the release.
- `release_finish(version)` — merge the release into `main` (`--no-ff`), tag
  `v{version}`, merge into `develop` (`--no-ff`), and delete the release branch.

`devflow ship` bumps the version, cuts the release branch from current HEAD,
commits the bump, and (unless `--no-pr`) pushes and opens a PR. `confirm` /
`rejectpr` finalize or undo a recorded ship (`.devflow/last-ship.json`).

## Configuration

`.devflow.yaml` is parsed by a small hand-rolled parser
(`crates/devflow-core/src/config.rs`); `Config::to_yaml()` is the canonical
schema (`devflow init` writes the defaults):

```yaml
version:
  scheme: semver
  file: Cargo.toml
  field: workspace.package.version
  build_number: git
automation:
  auto_branch: true
  auto_verify: true
  auto_docs: true
  auto_version: patch
  auto_ship: false
  auto_cleanup: true
  verify_command: "cargo test"
  lint_command: "cargo clippy -- -D warnings"
  docs_command: "cargo doc --no-deps 2>&1"
  continue_on_error: true
  docs_auto_commit: false
git_flow:
  main: main
  develop: develop
  feature_prefix: feature/
```

`git_flow` has exactly `main`, `develop`, and `feature_prefix` — there is no
`enabled` field.

## Logging

DevFlow uses the [`tracing`](https://docs.rs/tracing) ecosystem for structured
diagnostic logging. All log output goes to **stderr** via `tracing-subscriber`;
**stdout is reserved** for agent/system output and structured results.

### Configuration

| Environment variable | Purpose | Default |
|---|---|---|
| `RUST_LOG` | Controls log verbosity via `tracing-subscriber::EnvFilter`. Accepts bare levels (`error`, `warn`, `info`, `debug`, `trace`) or targeted directives (`devflow_core=debug,devflow=info`). | `info` |
| `DEVFLOW_LOG_FORMAT` | When set to `json`, enables JSON-structured log output (one JSON object per line on stderr). Any other value (or unset) produces human-readable plain-text logs. | plain text |

### Instrumentation

Key modules are instrumented with `tracing` spans and events:

- **`workflow.rs`** — State transitions emit `step_entered` / `step_exited` events
  at `INFO` level with `(before, after, phase)` fields. State I/O operations
  (`save_state`, `load_state`, `clear_state`) are logged at `DEBUG`.
- **`state.rs`** — `State::advance()` and `State::advance_skipping()` carry
  `#[tracing::instrument]` spans so call chains appear in log output.
- **`git.rs`** — Branch operations (`feature_start`, `feature_finish`) log at
  `INFO`; checkout and merge operations log at `DEBUG`; force operations log at
  `WARN`.
- **`monitor.rs`** — Agent spawn and PID tracking are logged at `INFO`; exit
  polling at `DEBUG`.
- **`ship.rs`** — Version bumps and PR creation log at `INFO`; confirm/reject
  at `WARN`.

### Log levels

| Level | Usage |
|---|---|
| `ERROR` | Unrecoverable conditions that abort an operation. |
| `WARN` | Recoverable anomalies (force operations, invalid config, stale state). |
| `INFO` | State transitions, agent lifecycle, branch/ship milestones. |
| `DEBUG` | I/O details, merge/checkout operations, exit polling. |
| `TRACE` | Fine-grained execution tracing (rarely needed outside debugging). |

### JSON output

When `DEVFLOW_LOG_FORMAT=json`, each log event is a single JSON line containing
`timestamp`, `level`, `target`, `fields`, and optional `span` information. This
format is designed for machine consumption (e.g., Hermes watching agent output
via structured logging).

```bash
# JSON log smoke test
DEVFLOW_LOG_FORMAT=json RUST_LOG=info cargo run -- status 2>devflow.json
head -1 devflow.json | python3 -m json.tool  # Should parse as valid JSON
```

`devflow doctor` includes a `RUST_LOG` environment check that validates the
variable is set to a parseable value and warns when it is missing or invalid.

## Extension points — adding an agent

DevFlow is agent-agnostic; agent-specific code lives only in `agents/*.rs` and
the targeted result parsing. Adding a backend is a checklist, not a fixed
"3 changes" — keep these in sync or tests/builds fail:

1. Add an adapter file under `crates/devflow-core/src/agents/` implementing the
   `Agent` trait.
2. Add a variant to the `AgentKind`/`Agent` enum in `state.rs`.
3. Update the `FromStr` parser, `Display`, and `AgentParseError` text in
   `state.rs` to accept/emit the new name.
4. Add a match arm in `agents::adapter_for()`.
5. Add the `pub mod` and `pub use` exports in `agents/mod.rs`.
6. Add/extend tests (adapter name, parser aliases, prompt-sharing).
7. Update docs (`README.md`, `CONTRIBUTING.md`, this file, dependency matrix).
