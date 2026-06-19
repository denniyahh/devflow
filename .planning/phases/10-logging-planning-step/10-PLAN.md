# Phase 10 Plan — Logging + Planning Step

**Status:** Ready for execution | **Target:** v1.3.0 | **Tests:** cargo test + clippy + fmt

## Execution Order

```
10a (Planning step in state machine) → 10b (Config: auto_plan toggle) → 10c (Logging: instrumentation)
→ 10d (Logging: JSON output + CLI init) → 10e (Doctor: RUST_LOG check) → 10f (Tests + docs)
```

10a must come first because all subsequent logging instrumentation of state transitions
depends on the new `Planning` step existing. 10b is next because the config toggle
controls whether `is_skippable()` applies. 10c–10d need 10a–10b in place. 10e adds
the doctor check once logging infrastructure exists. 10f wraps everything.

---

## 10a — Planning Step in State Machine

**Goal:** Add `Planning` variant to `Step` enum between `Branching` and `Executing`.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Add `Planning` variant to `Step` enum (between `Branching` and `Executing`) | `crates/devflow-core/src/state.rs` | Enum compiles, `#[serde(rename_all = "lowercase")]` handles serialization |
| 2 | Add `Planning` to `Step::next()` chain: `Branching → Planning → Executing` | `crates/devflow-core/src/state.rs` | `Step::Branching.next() == Some(Step::Planning)`, `Step::Planning.next() == Some(Step::Executing)` |
| 3 | Add `is_waiting()` returns `true` for `Planning` | `crates/devflow-core/src/state.rs` | `Step::Planning.is_waiting() == true` |
| 4 | Add `is_skippable()` returns `true` for `Planning` | `crates/devflow-core/src/state.rs` | `Step::Planning.is_skippable() == true` |
| 5 | Add `"planning"` to `Display` impl for `Step` | `crates/devflow-core/src/state.rs` | `Step::Planning.to_string() == "planning"` |
| 6 | Update `Config::should_skip()` to check `Step::Planning` against `auto_plan` toggle (return `true` when `auto_plan: true` — skips planning, auto-advances) | `crates/devflow-core/src/config.rs` | When `auto_plan: true`, `config.should_skip(&Step::Planning)` returns `true` |
| 7 | Update `status` CLI output: when step is `Planning`, show "awaiting plan review" | `crates/devflow-cli/src/main.rs` (`status` fn) | `devflow status` at Planning step shows "awaiting plan review" |
| 8 | Update `check` CLI: when step is `Planning` and `auto_plan: false`, pause (don't auto-advance); user must run `devflow check` explicitly to proceed | `crates/devflow-cli/src/main.rs` (`check` fn) | At Planning with `auto_plan: false`, `devflow check` advances only when user runs it |

**State chain after this area:**
```
Idle → Branching → Planning → Executing → Verifying → Docsing → Shipping → Cleaning
```

**Estimated:** 1 commit, ~60 lines changed.

---

## 10b — Config: `auto_plan` Toggle

**Goal:** Add `auto_plan: true` to `AutomationConfig` (default `false` — requires explicit `devflow check`).

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Add `auto_plan: bool` field to `AutomationConfig` struct, default `false` | `crates/devflow-core/src/config.rs` | Field exists, serde default `false` |
| 2 | Add `default_false()` serde default for `auto_plan` | `crates/devflow-core/src/config.rs` | `Config::default().automation.auto_plan == false` |
| 3 | Add `"auto_plan"` key to the YAML parser (`parse_config`) | `crates/devflow-core/src/config.rs` | `.devflow.yaml` with `automation:\n  auto_plan: true` parses correctly |
| 4 | Add `auto_plan` to `to_yaml()` output | `crates/devflow-core/src/config.rs` | Rendered YAML includes `auto_plan: false` |
| 5 | Add `should_skip` logic: when `auto_plan: true`, `Planning` step is skipped (auto-advance); when `false`, it blocks | `crates/devflow-core/src/config.rs` | See 10a task 6 |
| 6 | Add `auto_plan` to `parse_accepts_all_boolean_toggles` test | `crates/devflow-core/src/config.rs` | Test covers `auto_plan: true` and `auto_plan: false` |
| 7 | Add `auto_plan` to `defaults_match_documented_schema` test | `crates/devflow-core/src/config.rs` | `auto_plan` defaults to `false` |

**Estimated:** 1 commit, ~30 lines changed.

---

## 10c — Logging: Instrument Key Modules

**Goal:** Add `tracing` instrumentation to git, monitor, workflow, and state transition modules.

**Dependencies:** `tracing` and `tracing-subscriber` are already in `Cargo.toml` workspace deps. `tracing-subscriber::fmt::init()` is already called in `devflow-cli/src/main.rs:225`.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Add `use tracing::{info, debug, warn, error};` to `git.rs` | `crates/devflow-core/src/git.rs` | Imports compile |
| 2 | Instrument `GitFlow` methods: `info!` on branch create/delete, `debug!` on checkout/merge, `warn!` on force operations | `crates/devflow-core/src/git.rs` | `RUST_LOG=debug cargo run` shows git operations in stderr |
| 3 | Add `use tracing::{info, debug, warn};` to `monitor.rs` | `crates/devflow-core/src/monitor.rs` | Imports compile |
| 4 | Instrument `monitor::spawn_monitor`: `info!` on agent spawn/pid, `debug!` on exit polling | `crates/devflow-core/src/monitor.rs` | Monitor logs appear with `RUST_LOG=info` |
| 5 | Add `use tracing::{info, debug, warn};` to `workflow.rs` | `crates/devflow-core/src/workflow.rs` | Imports compile |
| 6 | Instrument `workflow::advance_state`: `info!` on each state transition with `(before, after, phase)` fields | `crates/devflow-core/src/workflow.rs` | State transitions logged to stderr |
| 7 | Instrument `workflow::save_state` / `load_state` / `clear_state`: `debug!` on I/O operations | `crates/devflow-core/src/workflow.rs` | File I/O logged at debug level |
| 8 | Add `use tracing::{info, debug, warn};` to `ship.rs` | `crates/devflow-core/src/ship.rs` | Imports compile |
| 9 | Instrument ship operations: `info!` on version bump, PR creation; `warn!` on confirm/reject | `crates/devflow-core/src/ship.rs` | Ship operations logged |
| 10 | Emit structured `tracing` events for state machine transitions: create a `step_entered` / `step_exited` event pattern using `tracing::event!` with `tracing::Level::INFO` | `crates/devflow-core/src/workflow.rs` | Structured events visible with `RUST_LOG=info` |
| 11 | Add `#[tracing::instrument]` to `State::advance()` and `State::advance_skipping()` | `crates/devflow-core/src/state.rs` | Span names appear in logs |

**Console output rule:** All logging goes to stderr via `tracing-subscriber`. Stdout is reserved for agent/system output.

**Estimated:** 1 commit, ~80 lines changed.

---

## 10d — Logging: JSON Output + CLI Init

**Goal:** Optional JSON log output for machine consumption (e.g., Hermes watching).

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | In CLI `main()`, read `DEVFLOW_LOG_FORMAT` env var. If `"json"`, configure `tracing_subscriber::fmt().json().init()` instead of plain `fmt().init()` | `crates/devflow-cli/src/main.rs` | `DEVFLOW_LOG_FORMAT=json RUST_LOG=info cargo run -- status 2>log.json` produces JSON lines |
| 2 | Default log format is plain text (current behavior); JSON only when `DEVFLOW_LOG_FORMAT=json` | `crates/devflow-cli/src/main.rs` | Default `cargo run` produces human-readable logs |
| 3 | Add `RUST_LOG` env var documentation: log levels are `error`, `warn`, `info`, `debug`, `trace`; default is `info` | `crates/devflow-core/src/lib.rs` doc comment or CLI help | `devflow --help` describes RUST_LOG |

**Estimated:** 1 commit, ~25 lines changed.

---

## 10e — Doctor: RUST_LOG Check

**Goal:** `devflow doctor` checks that `RUST_LOG` is set and parseable.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Add a `RUST_LOG` environment check to `doctor`: verify the env var is set, and if set, validate it parses as a valid `tracing::Level` or `tracing_subscriber::filter::EnvFilter` string | `crates/devflow-cli/src/main.rs` (doctor fn) | `RUST_LOG=info devflow doctor` shows "✓ RUST_LOG: info"; unset shows "✗ RUST_LOG: not set — defaulting to info" |
| 2 | If `RUST_LOG` is set to an invalid value, doctor warns: "RUST_LOG value may be invalid" | `crates/devflow-cli/src/main.rs` (doctor fn) | `RUST_LOG=bogus devflow doctor` shows warning |

**Estimated:** 1 commit, ~20 lines changed.

---

## 10f — Tests + Documentation

**Goal:** Full test coverage for new step, config, and logging; docs updated.

| # | Task | File(s) | Verification |
|---|---|---|---|
| 1 | Update `next_walks_full_chain_then_terminates` test to include `Planning` | `crates/devflow-core/src/state.rs` tests | Test passes with new chain |
| 2 | Update `only_executing_waits_on_an_agent` test — rename to reflect Planning also waits | `crates/devflow-core/src/state.rs` tests | Test passes, `Planning` asserts `is_waiting()` |
| 3 | Update `skippable_steps_are_verify_docs_ship` test — rename, add `Planning` as skippable | `crates/devflow-core/src/state.rs` tests | `Step::Planning.is_skippable() == true` |
| 4 | Update `step_display_is_lowercase` test — add `"planning"` | `crates/devflow-core/src/state.rs` tests | Assertion passes |
| 5 | Add test: `state_serde_round_trips_with_planning_step` — serialization round-trips `Planning` | `crates/devflow-core/src/state.rs` tests | JSON round-trip preserves `Planning` |
| 6 | Add test: `advance_state_with_auto_plan_skips_planning` — when `auto_plan: true`, advancing from `Branching` lands on `Executing` | `crates/devflow-core/src/workflow.rs` tests | Test passes |
| 7 | Add test: `advance_state_without_auto_plan_stops_at_planning` — when `auto_plan: false`, advancing from `Branching` lands on `Planning` | `crates/devflow-core/src/workflow.rs` tests | Test passes |
| 8 | Add test: `auto_plan_config_parses` — `.devflow.yaml` with `auto_plan: true` loads correctly | `crates/devflow-core/src/config.rs` tests | Test passes |
| 9 | Update `advance_state_saves_and_reports_transition` — account for Planning in the chain | `crates/devflow-core/src/workflow.rs` tests | Test passes with Planning in chain |
| 10 | Update `advance_state_skips_disabled_steps` — test Planning skip behavior | `crates/devflow-core/src/workflow.rs` tests | Test passes |
| 11 | Update `advance_skipping_jumps_over_disabled_steps` — test with `auto_plan: true` + `auto_verify: false` | `crates/devflow-core/src/state.rs` tests | Test passes |
| 12 | Update `advance_skipping_returns_to_idle_when_all_remaining_skipped` — test with all steps skipped including Planning | `crates/devflow-core/src/state.rs` tests | Test passes |
| 13 | Update `ARCHITECTURE.md` state machine diagram: add `Planning` between `Branching` and `Executing` | `ARCHITECTURE.md` | Diagram is accurate |
| 14 | Update `ARCHITECTURE.md` logging section: document `tracing` instrumentation, RUST_LOG, DEVFLOW_LOG_FORMAT | `ARCHITECTURE.md` | Section exists |
| 15 | Update `CONTRIBUTING.md` with logging conventions: use `tracing` macros, stderr for logs, structured events for state transitions | `CONTRIBUTING.md` | Section exists |
| 16 | Run full verification: `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check` | Terminal | All pass |

**Estimated:** 2 commits (tests + docs), ~200 lines changed.

---

## Verification

```bash
# Full test suite
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt -- --check

# Logging smoke test
RUST_LOG=debug cargo run -- status 2>devflow.log
grep -c "step_entered\|step_exited\|branch\|monitor" devflow.log

# JSON log test
DEVFLOW_LOG_FORMAT=json RUST_LOG=info cargo run -- status 2>devflow.json
head -1 devflow.json | python3 -m json.tool  # Should parse as valid JSON

# Doctor check
RUST_LOG=info cargo run -- doctor
```

## Deliverables

- `Planning` step in the workflow chain with config-controlled auto-advance
- `auto_plan` toggle in `.devflow.yaml` (default `false`)
- Structured tracing instrumentation across git, monitor, workflow, and ship modules
- State transition events (`step_entered`, `step_exited`) via tracing
- Optional JSON log output via `DEVFLOW_LOG_FORMAT=json`
- `devflow doctor` RUST_LOG environment check
- Full test coverage for new step + config + skip behavior
- Updated ARCHITECTURE.md and CONTRIBUTING.md

## Artifacts This Phase Produces

- **Enum variant:** `Step::Planning` (crates/devflow-core/src/state.rs)
- **Config field:** `AutomationConfig::auto_plan: bool` (crates/devflow-core/src/config.rs)
- **Config key:** `automation.auto_plan` in `.devflow.yaml` parser
- **Config method:** `Config::should_skip()` extended for `Step::Planning`
- **Tracing spans:** `State::advance`, `State::advance_skipping`
- **Tracing events:** `step_entered`, `step_exited` (workflow.rs)
- **CLI behavior:** `devflow status` "awaiting plan review" message at Planning step
- **CLI behavior:** `devflow check` pause-at-Planning logic
- **CLI behavior:** `main()` DEVFLOW_LOG_FORMAT env var detection
- **Doctor check:** RUST_LOG environment validation
- **Doc sections:** ARCHITECTURE.md state diagram update, logging section; CONTRIBUTING.md logging conventions
