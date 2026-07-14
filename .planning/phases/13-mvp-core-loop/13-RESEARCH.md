# Phase 13: MVP Core Loop - Research

**Researched:** 2026-07-14
**Domain:** Rust CLI orchestration — agent-process lifecycle, completion-protocol parsing, gate/notify mechanics, git worktrees. No new external dependencies.
**Confidence:** HIGH (codebase findings are direct source reads); MEDIUM (external CLI JSON envelope schemas — Anthropic/OpenAI docs have acknowledged gaps, see Sources)

<user_constraints>
## User Constraints (from CONTEXT.md)

CONTEXT.md for this phase is written as a scope contract (Goal + numbered sub-areas 13a–13e + explicit out-of-scope list), not the generic Decisions/Discretion/Deferred template. Reproduced verbatim below, section-mapped:

### Locked Decisions (from `## Goal`)

> Get the basic AI development loop — `devflow start` → Define→Plan→Code→Validate→Ship — working end-to-end so the operator can start using DevFlow on real projects again. Everything in this phase either unblocks the loop's terminal stage, fixes a correctness hole in the loop's success signal, or makes unattended daily use viable (never-silent failures, push notification on gates).

MVP scope decisions, confirmed with the operator:
- **Agents:** Claude Code + Codex. Hermes adapter deferred (Phase 14), Antigravity adapter deferred (Phase 15).
- **Gates:** answered via a pluggable notify hook (ntfy/desktop) — unattended runs, no terminal babysitting. Hermes plugin deferred (Phase 14).
- **Ship:** full loop including the automated Ship stage — the unclaimed `ship.rs` rewrite is claimed here.

### Sub-area scope (13a–13e, verbatim task lists)

**13a — Ship Stage GSD-Native Rewrite** (claims `11-VALIDATION.md` 11h-1…11h-4):
- [ ] Rewrite `ship_phase()` against the v2 GSD-native flow: `/gsd-ship` + `/gsd-code-review` integration
- [ ] `ReviewFailed` / `AgentFailed` handling in the Ship stage
- [ ] Remove or replace the v1 `LastShip` bookkeeping in `ship.rs` — written for the deleted `devflow confirm` / `devflow rejectpr` commands
- [ ] Tests for the Ship-stage paths (build on the 12-09 advance()/finish test harness)
- [ ] Re-run the Full-Ship manual verification recorded as **BLOCKED** in `12-12-SUMMARY.md` — it must pass before this phase closes

**13b — Completion Protocol Correctness** *(moved from 14a/14b)*:
- [ ] Verdict vs. ran split: `advance()`/`handle_validate_outcome()` treats `DEVFLOW_RESULT: success` from Validate as passed → Ship. An agent that runs `/gsd-validate-phase` and *finds gaps* succeeded at its task while validation failed. Add a distinct verdict field (e.g. `"verdict": "pass|gaps"`) OR evaluate Validate from the validation report artifact rather than the agent's self-report.
- [ ] Commit-count fallback (Layer 2) scoped to Code-like stages only — Define and Validate legitimately produce zero commits.
- [ ] Native envelope parsing: parse the Claude JSON envelope (`is_error`, `result`, `num_turns` — already requested via `--output-format json`, never parsed) and the Codex `--json` event stream as the authoritative per-adapter signals. `DEVFLOW_RESULT` marker stays as portable fallback.
- [ ] Tests: Validate-with-gaps does not advance to Ship; zero-commit Define/Validate runs don't trip Layer 2; envelope success/error per agent; fallback on absent/malformed envelope.

**13c — Never-Silent Loop** *(moved from 14d)*:
- [ ] WR-11 (Phase 11 code review): `advance()`'s catch-all arm for non-Validate stage failures (`main.rs:360-374`) returns an error but fires no gate and sends no notification — state left dirty with `gate_pending: false`, so nothing ever surfaces the halt. Route these through the same gate + notify path.
- [ ] Pluggable notify hook fired on gate-write (arbitrary shell command; operator will wire it to ntfy/desktop notification).
- [ ] `GATE_TIMEOUT_SECS` (`main.rs:16`, hardcoded 7 days) becomes configurable.
- [ ] Tests: stage failure fires gate + hook; hook failure is fail-soft (never blocks the loop).

**13d — Unattended-Safety Default** *(moved from 14c)*:
- [ ] Flip the default: `devflow start` uses a worktree unless an explicit opt-out flag is passed — the current default runs a `--dangerously-skip-permissions` agent directly in the primary checkout.
- [ ] Update CLI help text for the new default (full README rewrite stays in Phase 15); confirm worktree cleanup/`recover` paths handle the now-default case.

**13e — MVP Acceptance: Dogfood Run**:
- [ ] Full end-to-end run (Define→Plan→Code→Validate→Ship) on a real external project with the Claude adapter, gates answered via the notify hook
- [ ] Same loop exercised with the Codex adapter (at minimum through Code→Validate; confirm envelope parsing against the real `--json` stream)
- [ ] Any failure during dogfooding is in-scope for this phase — the loop working is the deliverable

### Claude's Discretion

Not called out as a distinct section in CONTEXT.md. Treat as discretionary (research-informed, not locked): exact env var names for the notify hook command and gate timeout override, exact shape of the `verdict` field/enum for 13b, whether `ReviewFailed` is a new `AgentStatus` variant or a `reason`-string convention, and how deeply `/gsd-code-review` gets woven into the Ship-stage prompt vs. left to the external `/gsd-ship` workflow's own optional-review step (see Summary — this is the single biggest open design question in the phase, flagged below).

### Deferred Ideas (OUT OF SCOPE, verbatim)

- Hermes agent adapter, Hermes skill-file rewrite, Hermes plugin — Phase 14 (moved from 15, 2026-07-14)
- Antigravity agent adapter, README/ARCHITECTURE rewrite, `.devflow.yaml` decoy removal, IN-01 rustdoc, `--help` snapshot CI test, dev container, CONTRIBUTING/CoC — Phase 15
- `devflow logs [--follow]`, `events.jsonl`, `devflow status` enrichment, `capture_agent_output()` sync-path decision — remain in Phase 14
- `devflow.toml` / configurable pipeline — still shelved per 2026-07-08 decision
- crates.io publish — publish-prep done in Phase 12; actual publish belongs with Phase 15 OSS readiness
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| 13a | Ship-stage GSD-native rewrite: remove dead v1 `LastShip`/PR-body bookkeeping, add `ReviewFailed`/`AgentFailed` handling to the Ship-stage outcome path, re-verify Full-Ship | See "13a — Ship Stage Reality Check" below: `ship_phase()` as a discrete function does not need to (and should not) reimplement `gh pr create` — that already runs inside the external `/gsd-ship` slash command the Ship-stage agent executes. The real gap is in `main.rs`'s `handle_ship_outcome`/`advance()`, which has no failure branch for the Ship stage at all today. |
| 13b | Completion-protocol correctness: verdict-vs-ran split, Layer-2 scoping, native Claude/Codex envelope parsing | See "13b — Completion Protocol" below: exact current parsing code (`agent_result.rs`), exact envelope fields confirmed against current Anthropic/OpenAI docs, and the specific gap (`is_error` is never read). |
| 13c | Never-silent: WR-11 catch-all fix, notify hook, configurable `GATE_TIMEOUT_SECS` | See "13c — Never-Silent Loop" below: exact line/arm in `main.rs`, existing shell-invocation idiom to reuse (`hooks.rs::docs_update`), existing env-var convention to reuse (`DEVFLOW_LOG_FORMAT`). |
| 13d | Worktree-by-default | See "13d — Worktree Default" below: exact CLI flag definition, `worktree::add`/`recover.rs` behavior already agent/worktree-path-aware. |
| 13e | Dogfood acceptance run | See "Validation Architecture" and "13e — Dogfood Run Requirements" below: what must be true of 13a–13d before a real run can be attempted, and what to capture as evidence. |
| WR-11 | Silent-halt fix for non-Validate stage failures in `advance()`'s catch-all arm | Same as 13c — WR-11 *is* 13c's headline item, not a separate concern. |
</phase_requirements>

## Summary

This phase is pure Rust remediation inside an already-mature, well-tested workspace (`devflow-core` + `devflow-cli`, 2 crates, no new dependencies needed). All five sub-areas touch the same small set of files: `crates/devflow-cli/src/main.rs` (the `advance()`/`handle_ship_outcome()`/`handle_validate_outcome()` state-machine functions), `crates/devflow-core/src/{ship,agent_result,gates,prompt,worktree}.rs`, and `crates/devflow-core/src/agents/{claude,codex}.rs`.

The single most important finding, which should reshape how the planner scopes 13a: **DevFlow's own `ship.rs` code does not create PRs and never did in the current (v2 GSD-native) architecture.** The actual `git push` + `gh pr create` + optional-review flow lives entirely inside the external GSD slash command `/gsd-ship` (defined in `~/.claude/gsd-core/workflows/ship.md`, outside this repo, executed by whichever coding agent the Ship stage launches). DevFlow's role at the Ship stage is: (1) launch an agent with a prompt instructing it to run `/gsd-ship {N}` (`prompt::stage_prompt(Stage::Ship, ...)`), (2) wait for it to exit, (3) parse its `DEVFLOW_RESULT` marker / JSON envelope, (4) fire a human-approval gate ("Ship complete — approve merge?"), (5) run terminal hooks (version bump, branch cleanup) and clear state. `ship.rs` today contains only dead v1 bookkeeping (`LastShip`, `build_pr_body`, `extract_goal`, `count_passed_tests`, `mark_phase_complete` — confirmed via grep, zero call sites outside `ship.rs`'s own tests) left over from the deleted `devflow confirm`/`devflow rejectpr` commands, plus two things that *are* still live: `prepend_changelog` (used by `hooks.rs`) and the `CronInstructions`/cron-schedule machinery (used by `sequentagent`). So "rewrite `ship_phase()`" should not be read as "devflow must reimplement PR creation" — it should be read as "delete the dead PR/LastShip code, and give `main.rs`'s Ship-stage outcome handling the same rigor Validate already has" (specifically: a real failure path, since today a failed Ship-stage agent run falls into the exact same silent-halt bug WR-11 names for every non-Validate stage).

This directly explains why 13a and 13c overlap: `handle_ship_outcome()` today only handles the *success* path of the Ship stage (it calls `run_gate` unconditionally, assuming the agent succeeded). A Ship-stage agent failure (e.g. `/gsd-ship` couldn't push, `gh` not authenticated, or the external workflow's own review step found REVISE-level issues) is not distinguished from success anywhere — `advance()`'s `failed` check only special-cases `Stage::Validate`; every other stage including `Stage::Ship` falls into the catch-all `_ => Err(...)` arm at `main.rs:360-374`, which WR-11 already flags as silent (no gate, no notification, `gate_pending` left `false`). Fixing WR-11 (13c) and giving Ship a real failure branch (13a) are the same code change viewed from two angles. The `ReviewFailed`/`AgentFailed` distinction CONTEXT.md asks for maps naturally onto: `ReviewFailed` (the agent ran `/gsd-code-review` or the ship workflow's own review step and found real issues — should loop back to Code, like a Validate failure) vs. `AgentFailed` (the agent process itself errored — push failed, `gh` missing, envelope reported an error — should hit the WR-11 gate+notify path, never silent).

For 13b, the concrete gap is narrow and already partially built: `agent_result.rs::detect_rate_limit` already parses several Claude JSON-envelope fields recursively (`subtype`, `api_error_status`, `status`, `status_code`) but **never reads the top-level `is_error` boolean** — the single field Anthropic's own docs name as the authoritative error signal. `extract_json_result_text` already unwraps the `result` field to search for the `DEVFLOW_RESULT` marker inside it, but nothing reads `num_turns` or treats `is_error: true` as a Layer-1 failure signal independent of whether a `DEVFLOW_RESULT` marker is present. For Codex, the adapter already passes `--json` but nothing parses the resulting JSONL event stream (`thread.started`/`turn.started`/`item.*`/`turn.completed`/`turn.failed`) at all — today Codex output is only searched as flat text for a `DEVFLOW_RESULT:` line or a "try again at" substring.

For 13c, the codebase already has the exact idiom to copy for a "pluggable shell command" notify hook: `hooks.rs::docs_update` and `ship.rs::test_summary` both already shell out via `Command::new("sh").arg("-c").arg(cmd)`, fail-soft on error. `DEVFLOW_LOG_FORMAT` is the established env-var-for-runtime-config convention (there is deliberately no config file — `.devflow.yaml`/`devflow.toml` were explicitly eliminated/shelved per STATE.md decisions of 2026-06-19 and 2026-07-08). A new `DEVFLOW_GATE_NOTIFY_CMD` (or similar) env var read at the `run_gate` call site is the pattern-consistent choice, not a CLI flag or config file.

For 13d, flipping the worktree default is almost entirely a CLI-layer change: `Start`'s `worktree: bool` field just needs its default inverted (with an opt-out flag), and `ensure_phase_worktree`/`worktree::add`/`recover.rs`'s stale-state detection are already worktree-path-aware (state already carries `worktree_path: Option<PathBuf>` and `agent_pid_for`/`is_stale_state` already work regardless of whether the agent ran in the main checkout or a worktree). The main risk is `--dangerously-skip-permissions` in `agents/claude.rs::exec_command` being unconditional — that flag stays (it's what makes headless unattended runs possible at all; the worktree is the isolation boundary that makes it *safe*, not something to remove).

**Primary recommendation:** Scope 13a as "delete dead ship.rs bookkeeping + add a real Ship-stage failure/review-outcome branch to `main.rs`" (not "reimplement PR creation"); scope 13b as "add `is_error`/`num_turns` reads to the Claude envelope parser and a new Codex JSONL event-stream parser, both feeding into a stage-scoped Layer-2 gate"; scope 13c and 13a's failure-handling as one connected change; scope 13d as a CLI default flip plus help-text/doc update; treat 13e as a manual verification task gated on 13a–13d all being merged and green.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Stage-machine orchestration (advance, transitions, gates) | CLI (`devflow-cli::main.rs`) | Core (`devflow-core::workflow`/`stage`/`mode`) | `main.rs` owns the decision logic; core owns persisted state and pure helpers. |
| PR creation, `gh pr create`, code review dispatch | External agent process (GSD slash-command layer, outside this repo) | — | `/gsd-ship`/`/gsd-code-review` run *inside* the launched coding agent's own turn, not in DevFlow's Rust code. DevFlow only launches the agent and reads its exit signal. |
| Agent completion-signal parsing (DEVFLOW_RESULT, JSON envelopes) | Core (`devflow-core::agent_result`) | — | Pure parsing logic, already covered by extensive unit tests; no I/O beyond reading a captured file. |
| Gate file protocol (write/poll/ack/cleanup) | Core (`devflow-core::gates`) | CLI (`run_gate` orchestration) | Gates own file I/O + atomic writes; CLI decides *when* to gate via `Mode::should_gate`. |
| Notify hook (new, 13c) | CLI (`run_gate` call site in `main.rs`) | Core (a small `notify` module, or inline in `gates.rs`) | Should fire at the exact point a gate file is written, mirroring how `hooks.rs` hooks fire at stage transitions — shell-out, fail-soft, same idiom as `docs_update`. |
| Worktree lifecycle (create/remove/prune/list) | Core (`devflow-core::worktree`) | CLI (`ensure_phase_worktree`, `Start` flag) | Core already fully implements git-worktree mechanics; CLI only decides *whether* to call it (13d flips this default). |
| Agent process launch/capture | Core (`devflow-core::agent`, `devflow-core::monitor`) | — | Spawns headless CLIs, captures stdout/exit code to `.devflow/`; agent-specific flags live in `devflow-core::agents::{claude,codex}`. |

## Standard Stack

### Core

No new external crates are needed for any of 13a–13d — all required behavior (shell-out for the notify hook, env-var config, JSON parsing for Codex's event stream, additional envelope fields) is achievable with dependencies already in the workspace.

| Library | Version (resolved, `Cargo.lock`) | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde` / `serde_json` | 1.0.228 / 1.0.150 [VERIFIED: Cargo.lock] | Envelope/event JSON parsing, `AgentResult` (de)serialization | Already the workspace's only JSON stack; every module (`agent_result.rs`, `gates.rs`, `state.rs`) uses it |
| `thiserror` | 2.0.18 [VERIFIED: Cargo.lock] | Error enums (`ShipError`, `GateError`, `AgentError`, `CliError`) | Existing convention across every module in the workspace |
| `tracing` | 0.1.44 [VERIFIED: Cargo.lock] | Structured logging for the new notify-hook and Ship-failure paths | Existing convention (`info!`/`warn!` in `hooks.rs`, `gates.rs`) |
| `libc` | 0.2.186 [VERIFIED: Cargo.lock] | `kill(pid, 0)` liveness check (already used by `agent.rs::agent_running`, relevant to 13d's worktree-default interplay with `recover.rs`) | Already the workspace's only process-liveness primitive |
| `clap` | 4.6.1 [VERIFIED: Cargo.lock] | `Start`'s worktree flag flip (13d) is a `#[arg]` default-value change | Already the CLI's only arg-parsing crate |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tempfile` | 3.27.0 [VERIFIED: Cargo.lock] (dev-dependency) | All new tests (Ship-stage outcome, notify-hook fail-soft, Codex event parsing) should use `tempfile::tempdir()` exactly like every existing test in this workspace | Test-only |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Env var for notify-hook command / gate timeout | A `devflow.toml` config file | Explicitly shelved per STATE.md (2026-06-19 "Config eliminated", reaffirmed 2026-07-08 "deliberately shelved — not part of Phase 13 or 14"). Do not reopen this decision inside 13c; env vars are the only pattern-consistent option available to this phase. |
| Hand-rolled Codex JSONL line-by-line parser | A generic NDJSON crate | Unnecessary — `serde_json::Deserializer::from_str(..).into_iter::<Value>()` or a simple `.lines().map(serde_json::from_str)` loop is sufficient for the small, already-`serde_json`-using codebase; do not add a new dependency for this. |

**Installation:** None — no new dependencies to add to either `Cargo.toml`.

## Package Legitimacy Audit

**Not applicable — this phase introduces no new external packages.** All functionality is built from dependencies already present in the workspace (`serde`, `serde_json`, `thiserror`, `tracing`, `libc`, `clap`), confirmed via direct `Cargo.lock` inspection. If the planner introduces any new crate during planning (none identified as necessary in this research), it must be run through the Package Legitimacy Gate before being added.

## Architecture Patterns

### System Architecture Diagram

```
 devflow start --phase N [--no-worktree]  (13d: worktree becomes default)
        │
        ▼
 ┌─────────────────────┐        writes .devflow/state.json (Stage::Define)
 │   CLI: start()       │───────────────────────────────────────────────┐
 └─────────────────────┘                                                │
        │ launch_stage()                                                ▼
        ▼                                                    ┌────────────────────┐
 ┌─────────────────────┐   spawns detached monitor            │ .devflow/state.json │
 │ monitor::spawn_monitor│──────────┐                          └────────────────────┘
 └─────────────────────┘           │
                                    ▼
                     ┌───────────────────────────┐
                     │ agent process (claude -p / │  runs the GSD slash command for
                     │ codex exec), captured to   │  the stage: /gsd-discuss-phase,
                     │ .devflow/phase-NN-stdout   │  /gsd-plan-phase, /gsd-execute-phase,
                     └───────────────────────────┘  /gsd-validate-phase, or /gsd-ship
                                    │ exits
                                    ▼
                     ┌───────────────────────────┐
                     │ devflow advance            │  (13b) evaluate_agent_result():
                     │  = agent_result::evaluate_  │   Layer 1: DEVFLOW_RESULT marker /
                     │    agent_result()           │            native envelope (is_error,
                     └───────────────────────────┘             num_turns, Codex event stream)
                                    │                Layer 2: exit-code + commit-count
                                    │                         (13b: Code-like stages only)
                    ┌───────────────┼────────────────────┐   Layer 3: process-gone fallback
                    ▼               ▼                    ▼
              Stage::Define/    Stage::Validate      Stage::Ship
              Plan/Code           │                      │
              success → next   handle_validate_       handle_ship_outcome()
              stage             outcome()               (13a: needs a real failure/
              FAILURE today  →  loops to Code or        ReviewFailed/AgentFailed branch —
              silent halt       gates (existing)        today only handles success)
              (WR-11 / 13c)        │                      │
                    │               ▼                    ▼
                    │         run_gate() ───────►  .devflow/gates/NN-{stage}.json
                    │               │              (13c: notify hook fires HERE)
                    │               ▼
                    └────────►  Gates::poll_response() ◄── human/Hermes writes
                                    │                       NN-{stage}.response.json
                                    ▼
                          GateAction::{Advance,LoopBack,Abort}
                                    │
                                    ▼
                     finish_workflow() — VersionBump, BranchCleanup, clear state
```

Note: `/gsd-ship` and `/gsd-code-review` (the box labeled "runs the GSD slash command for the stage") execute entirely *inside* the agent process box — DevFlow's Rust code never calls `gh` directly. This is the key boundary the planner must respect: 13a's code changes live in the `handle_ship_outcome`/`advance` box, not in a new `gh pr create` wrapper.

### Recommended Project Structure

No new files are required; all changes fit into existing modules:

```
crates/devflow-cli/src/main.rs        # advance() failure branch for Ship (13a/13c);
                                       # run_gate() notify-hook call site (13c);
                                       # GATE_TIMEOUT_SECS → env-var-overridable const (13c);
                                       # Start.worktree default flip (13d)
crates/devflow-core/src/
├── ship.rs        # DELETE: LastShip, save/load/delete, build_pr_body, extract_goal,
                    #         extract_section, changed_files, test_summary, count_passed_tests,
                    #         mark_phase_complete (all confirmed dead — 11h-2)
                    # KEEP: CronInstructions/*, prepend_changelog, shell_quote,
                    #       cron_schedule_from_retry_after + its date-math helpers
├── agent_result.rs # ADD: is_error/num_turns reads for Claude envelope (13b);
                    #      Codex JSONL event-stream parser (13b);
                    #      stage-aware Layer-2 gate (skip Define/Validate, 13b)
├── prompt.rs       # Possibly extend Ship-stage prompt to name /gsd-code-review
                    #   explicitly and define the ReviewFailed contract (13a, discretionary)
├── gates.rs        # Possibly host the notify-hook trigger point (13c, or keep in main.rs)
├── state.rs        # Possibly add a `verdict` field or reuse `AgentResult.reason` string
                    #   convention for Ship ReviewFailed/AgentFailed (13a/13b, discretionary)
└── worktree.rs, recover.rs  # No structural change expected — already worktree-path-agnostic;
                              # 13d should only need CLI-layer + doc changes
```

### Pattern 1: Fail-soft shell-out hook (reuse for the 13c notify hook)
**What:** Run an external command via `sh -c`, log success/failure, never propagate the error upward.
**When to use:** The notify hook must never block or fail the pipeline — exactly the existing `DocsUpdate` hook's contract.
**Example:**
```rust
// Source: crates/devflow-core/src/hooks.rs (existing code, docs_update)
fn docs_update(ctx: &HookContext) -> Result<(), HookError> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("cargo doc --no-deps 2>&1")
        .current_dir(&ctx.project_root)
        .output();
    match output {
        Ok(out) if out.status.success() => { /* ... */ }
        Ok(_) => warn!("DocsUpdate: cargo doc reported a failure; skipping commit"),
        Err(err) => warn!("DocsUpdate: could not run cargo doc: {err}"),
    }
    Ok(()) // always Ok — fail-soft
}
```
A 13c notify hook should follow this exact shape: read the command from an env var (e.g. `DEVFLOW_GATE_NOTIFY_CMD`), skip silently if unset, run it via `sh -c` with the gate's phase/stage/context piped in (stdin or args), log a `warn!` on non-zero exit or spawn failure, and always return `Ok(())`/never abort `run_gate`.

### Pattern 2: Env-var-driven runtime config (no config file)
**What:** `std::env::var("DEVFLOW_LOG_FORMAT")` gates JSON vs. text log output in `main.rs::main()`.
**When to use:** Any per-run behavior toggle that must not resurrect the shelved `devflow.toml` decision — applies directly to 13c's `GATE_TIMEOUT_SECS` override and notify-hook command.
**Example:**
```rust
// Source: crates/devflow-cli/src/main.rs (existing code)
match std::env::var("DEVFLOW_LOG_FORMAT").as_deref() {
    Ok("json") => { tracing_subscriber::fmt().json().init(); }
    _ => { tracing_subscriber::fmt::init(); }
}
```
Apply the same shape for `GATE_TIMEOUT_SECS`: read `DEVFLOW_GATE_TIMEOUT_SECS`, parse as `u64`, fall back to the existing `7 * 24 * 60 * 60` constant on absence or parse failure.

### Pattern 3: Stage-scoped failure handling (existing shape to extend for Ship)
**What:** `handle_validate_outcome()` already implements exactly the "loop back vs. gate vs. abort" logic 13a needs for Ship's `ReviewFailed`/`AgentFailed` split.
**When to use:** Model the new Ship-failure branch directly on this function rather than inventing a new pattern.
**Example:**
```rust
// Source: crates/devflow-cli/src/main.rs (existing code, handle_validate_outcome — the
// template to adapt for Ship's ReviewFailed/AgentFailed distinction)
fn handle_validate_outcome(
    project_root: &Path,
    state: &mut State,
    passed: bool,
) -> Result<(), CliError> {
    if !passed {
        state.consecutive_failures += 1;
        workflow::save_state(state)?;
    }
    if state.mode.should_gate(Stage::Validate, state.consecutive_failures) {
        // ... run_gate(), match GateAction ...
    }
    if passed { transition(project_root, state, Stage::Ship) }
    else { loop_back_to_code(project_root, state) }
}
```

### Anti-Patterns to Avoid
- **Reimplementing PR creation in `ship.rs`:** The external `/gsd-ship` workflow already does `git push` + `gh pr create` + PR-body generation inside the agent's own turn. Duplicating this in devflow's Rust code would create two sources of truth for PR body content and is explicitly *why* `build_pr_body`/`extract_goal` are dead code today (11h-2) — do not resurrect this pattern under a new name.
- **Adding a config file for the notify hook / gate timeout:** Directly contradicts the 2026-06-19/2026-07-08 "config eliminated / shelved" decisions in STATE.md. Use env vars.
- **Trusting `DEVFLOW_RESULT` alone for the Ship stage without also checking the native envelope's `is_error`:** An agent can print `DEVFLOW_RESULT: {"status":"success"}` after silently failing to push (if its prompt doesn't explicitly instruct it to check `gh pr create`'s exit code) — cross-checking `is_error`/exit code closes exactly the "verdict vs. ran" gap 13b names for Validate, and the same risk applies to Ship.
- **Removing `--dangerously-skip-permissions` from `agents/claude.rs` as part of 13d:** The worktree is the isolation boundary; the permission-bypass flag is what makes headless unattended runs possible at all. 13d's job is to make the worktree the default, not to remove the flag.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Shelling out to a notify command | A new process-spawn abstraction | `std::process::Command::new("sh").arg("-c")` exactly as `hooks.rs::docs_update` and `ship.rs::test_summary` already do | Two existing call sites already establish the idiom; a third inconsistent one adds no value |
| NDJSON/event-stream parsing for Codex | A hand-rolled streaming JSON tokenizer | `serde_json` line-by-line (`stdout.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())`) since the full Codex stdout is already captured to a file before `advance` runs (not truly streamed) | DevFlow's capture model (`agent.rs::capture_agent_output`) already buffers full stdout to a string before advance() runs — there is no live-stream requirement, just a batch line-by-line parse over already-captured text |
| Atomic gate/state file writes | A new "safe write" helper for the notify-hook or new Ship fields | `write_atomic`/`write_state_atomic` (temp-file + rename) already implemented in `gates.rs` and `workflow.rs` | Exact same crash-safety requirement already solved twice in this codebase |
| Env-var parsing with fallback | A generic config-loading crate | `std::env::var(...).ok().and_then(|s| s.parse().ok()).unwrap_or(default)` | One-line pattern already used for `DEVFLOW_LOG_FORMAT`; no need for `config`/`figment`/etc. crates for two env vars |

**Key insight:** Every mechanical building block 13a–13d need (shell-out, atomic write, env-var config, stage-scoped state transition) already exists once or twice in this codebase. The work is almost entirely *extending existing functions* (`advance`, `handle_validate_outcome`-shaped logic, `agent_result::evaluate_layer1/2`) and *deleting dead code* (`ship.rs`'s v1 bookkeeping), not introducing new architecture.

## Common Pitfalls

### Pitfall 1: Treating "Ship stage succeeded" the same as "PR was actually created"
**What goes wrong:** The Ship-stage agent's `DEVFLOW_RESULT: success` only proves the agent's *own* turn ended without it reporting failure — it does not prove `/gsd-ship`'s `gh pr create` step actually ran or succeeded (e.g. the AskUserQuestion-driven "optional_review" step in `ship.md` is interactive and has no defined behavior when there's no human to answer it in a headless `--dangerously-skip-permissions` run).
**Why it happens:** `handle_ship_outcome()` today calls `run_gate` unconditionally on any exit from the Ship-stage agent, assuming success.
**How to avoid:** Cross-check the native envelope's `is_error`/exit code (13b) at the Ship stage too, and treat an agent that never emitted a marker as `AgentFailed`, not silent success.
**Warning signs:** A "Ship complete — approve merge?" gate firing when no PR actually exists on GitHub.

### Pitfall 2: `/gsd-ship`'s interactive `AskUserQuestion` review step in headless/auto mode
**What goes wrong:** `ship.md`'s `optional_review` step always prompts (`AskUserQuestion: "PR created. Run a code review before merge?"`) unless a `workflow.code_review_command` is configured. In a `claude -p --dangerously-skip-permissions` headless run there is no human to answer this — behavior is undefined/untested from DevFlow's side.
**Why it happens:** The external GSD workflow was designed for interactive use; DevFlow's headless dogfood run (13e) is a different execution mode than the workflow authors likely tested against.
**How to avoid:** For 13e's dogfood run, either configure `workflow.code_review_command` (external review command) ahead of time, or explicitly verify what Claude/Codex do when they hit an `AskUserQuestion` with no available answer in headless mode, and record the outcome. This is a **manual verification item**, not something Rust code can fix — flag it for the planner as a pre-flight check before attempting 13e.
**Warning signs:** The Ship-stage agent hangs (monitor never sees exit) or silently picks a default that the operator didn't intend.

### Pitfall 3: WR-11's fix breaking the existing Validate-specific failure tests
**What goes wrong:** `advance()`'s `match stage { Stage::Validate => ..., _ => Err(...) }` is exactly where WR-11 lives. A naive fix that routes *all* non-Validate failures through `handle_validate_outcome`-style gating risks changing Validate's own already-tested behavior if the refactor isn't kept stage-parametric.
**Why it happens:** The cleanest fix factors gate-on-failure logic out of `handle_validate_outcome` into a shared helper parametrized by stage — but Validate's `consecutive_failures`/auto-loop semantics (`Mode::should_auto_loop`) are Validate-specific and must not leak onto Define/Plan/Code/Ship.
**How to avoid:** Keep the new "always gate on failure for non-Validate stages" path structurally separate from `handle_validate_outcome`'s loop-back logic — a new small function (e.g. `handle_stage_failure`) that always fires a gate + notify hook, never auto-loops, is safer than trying to generalize `handle_validate_outcome`.
**Warning signs:** Existing tests `validate_failure_threshold_forces_gate_then_aborts` / `abort_cleans_up_gate_files_so_a_later_gate_does_not_reuse_stale_response` regressing.

### Pitfall 4: Codex `--json` event stream isn't the same shape as Claude's single envelope
**What goes wrong:** Claude's `--output-format json` produces exactly one JSON object at the end of stdout; Codex's `--json` produces one JSON object *per line* (JSONL) throughout execution, including `item.started`/`item.updated` progress events interleaved with the final `turn.completed`/`turn.failed`. A parser written assuming "the whole stdout is one JSON document" (as `agent_result.rs::extract_json_result_text` currently does via `serde_json::from_str(trimmed)`) will fail silently (return `None`) on Codex output, because Codex's captured stdout is not valid as a single JSON document.
**Why it happens:** The two agents' `--json`/`--output-format json` flags are superficially similar names for structurally different formats.
**How to avoid:** Write a distinct Codex-specific parser that iterates lines, parses each independently, and looks for the last `turn.completed`/`turn.failed` event — do not try to reuse `extract_json_result_text`'s single-document assumption for Codex.
**Warning signs:** Codex-adapter tests passing while a real Codex run's envelope parsing silently falls through to Layer 2/3.

### Pitfall 5: Exact Claude/Codex JSON error-subtype enumeration is not fully documented upstream
**What goes wrong:** Beyond `subtype: "success"` and the rate-limit-related fields already handled, Anthropic's own CLI reference does not fully enumerate all `subtype` error values for `claude -p --output-format json` (confirmed via two currently-open Anthropic GitHub doc-gap issues, see Sources) — anything inferred about non-success/non-rate-limit subtypes is necessarily `[ASSUMED]`, not `[CITED]`.
**Why it happens:** Anthropic and OpenAI's docs prioritize the interactive/streaming and SDK use cases; the plain single-envelope `--output-format json` and Codex's JSONL event schema are both marked as under-documented by the vendors' own tracked issues as of this research date.
**How to avoid:** Treat `is_error: bool` as the primary, authoritative signal (documented, stable) and any specific `subtype` string values beyond `"success"`/`"error_rate_limit"` as best-effort/defensive, not load-bearing. The 12-12-SUMMARY.md manual verification already demonstrated the empirically-safe approach: make one real API call, inspect the actual JSON, and write the parser against what's actually observed rather than the full hypothetical schema.
**Warning signs:** A parser that special-cases many specific subtype strings and silently mis-classifies a legitimate but undocumented subtype as success or failure.

## Code Examples

### Existing Claude/Codex adapter definitions (unchanged surface, 13b builds on top)
```rust
// Source: crates/devflow-core/src/agents/claude.rs (existing code)
fn exec_command(&self, _phase: u32, prompt: &str) -> (&'static str, Vec<String>) {
    ("claude", vec![
        "-p".into(), prompt.to_string(),
        "--output-format".into(), "json".into(),
        "--dangerously-skip-permissions".into(),
    ])
}
```
```rust
// Source: crates/devflow-core/src/agents/codex.rs (existing code)
fn exec_command(&self, _phase: u32, prompt: &str) -> (&'static str, Vec<String>) {
    ("codex", vec![
        "exec".into(), "--sandbox".into(), "workspace-write".into(),
        "--json".into(), prompt.to_string(),
    ])
}
```

### Existing envelope-unwrap logic to extend with `is_error`/`num_turns` (13b)
```rust
// Source: crates/devflow-core/src/agent_result.rs (existing code)
fn extract_json_result_text(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') { return None; }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
    value.get("result")?.as_str().map(str::to_string)
}
```
Extend this (or add a sibling function) to also read `value.get("is_error").and_then(Value::as_bool)` and `value.get("num_turns").and_then(Value::as_u64)` — both top-level, sibling fields to `result` on the same envelope object, per the confirmed schema: `{"type":"result","subtype":"success","is_error":false,"duration_ms":2847,"duration_api_ms":1923,"num_turns":4,"result":"...","total_cost_usd":0.0034,"session_id":"..."}` [CITED: code.claude.com/docs/en/cli-reference + community documentation of the JSON output format, cross-referenced].

### Confirmed Codex JSONL event shapes (13b, new parser needed)
```json
{"type":"thread.started","thread_id":"0199a213-81c0-7800-8aa1-bbab2a035a53"}
{"type":"turn.started"}
{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"bash -lc ls","status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_3","type":"agent_message","text":"..."}}
{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122,"reasoning_output_tokens":0}}
```
A `turn.failed` event (not shown above but documented) carries an `error` object with a `message` field. [CITED: developers.openai.com/codex/noninteractive → learn.chatgpt.com/docs/non-interactive-mode, cross-referenced against github.com/openai/codex docs/exec.md and a third-party "exec --json event cheatsheet"]. Parse strategy: split captured stdout on newlines, `serde_json::from_str::<Value>` each non-empty line (skip parse failures — Codex progress lines are not part of the completion contract), find the **last** `turn.completed` or `turn.failed` event, and treat `turn.failed` as `AgentStatus::Failed` with `reason` from `error.message`.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `devflow ship` writes `LastShip`; `devflow confirm`/`devflow rejectpr` read it back to finalize/reject a PR | `/gsd-ship` (external GSD slash command) handles push + PR creation + review entirely within one agent turn; DevFlow only orchestrates the stage and reads the completion signal | Phase 11 GSD-native rewrite (2026-06-20) removed `devflow confirm`/`devflow rejectpr`; `ship.rs`'s bookkeeping for them was never cleaned up | `ship.rs` carries ~250 lines of dead code (confirmed via grep: zero non-test call sites) that 13a should delete |
| Ship stage had no distinct failure handling | Every other gate stage (Validate) already has full loop-back/gate/abort handling; Ship still assumes success | Validate's handling was built out across Phase 11/12; Ship was never revisited | 13a/13c's core work: bring Ship's failure handling up to Validate's existing standard |
| `DEVFLOW_RESULT` marker was the only completion signal | Claude/Codex both already support richer native JSON signals (`is_error`, `num_turns`, Codex's event stream) that were requested (flags already present) but never parsed | Recognized in the 2026-07-08 external code review that scoped old Phase 14 | 13b closes a request/parse mismatch that has existed since the flags were first added |

**Deprecated/outdated:**
- `LastShip`, `devflow confirm`, `devflow rejectpr`: fully removed from the CLI surface in Phase 11; `ship.rs`'s supporting code for them is dead and should be deleted in 13a, not preserved "just in case."
- `.devflow.yaml`/`devflow.toml` as a configuration mechanism: explicitly eliminated (2026-06-19) and later reconsidered-then-shelved (2026-07-08) — do not use for 13c's new configurables.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The `ReviewFailed`/`AgentFailed` split should map onto (loop-back-to-Code) vs. (gate+notify), mirroring Validate's existing loop-back/gate logic | Summary, Pattern 3 | If the operator actually wants a different Ship-failure UX (e.g. always gate, never auto-loop back to Code from Ship), the planner should confirm this design choice explicitly — CONTEXT.md names the two states but not the exact transition each should trigger. |
| A2 | Env vars (`DEVFLOW_GATE_NOTIFY_CMD`, `DEVFLOW_GATE_TIMEOUT_SECS` or similar names) are the right mechanism for 13c's two new configurables | Standard Stack (Alternatives), Pattern 2 | Low risk — directly supported by two explicit prior decisions in STATE.md, but exact env var names are not specified anywhere and are a planner/discuss-phase decision point. |
| A3 | Claude's `--output-format json` envelope's exact non-success `subtype` values beyond `"success"`/`"error_rate_limit"` are not fully enumerable from current public docs | Pitfall 5, Code Examples | If the planner writes exhaustive subtype-matching logic based on this research, it may miss real subtype values Anthropic hasn't documented; prefer `is_error` as the primary signal. |
| A4 | Codex's captured stdout can be parsed as buffered JSONL (not truly streamed) because DevFlow's `capture_agent_output` already reads the full stdout to a string before `advance` runs | Don't Hand-Roll, Pitfall 4 | Confirmed true by direct reading of `agent.rs::capture_agent_output` [VERIFIED: source] — low risk, but worth planner awareness that no async/streaming parser is needed. |
| A5 | The `/gsd-ship` workflow's interactive `AskUserQuestion` review step has undefined behavior under `--dangerously-skip-permissions` headless execution with no human present | Pitfall 2 | This is untested from DevFlow's side (12-12-SUMMARY.md's Task 2 explicitly avoided `--dangerously-skip-permissions` for exactly this class of risk). 13e's dogfood run may surface a real hang here; the planner should treat this as a pre-flight risk to investigate before attempting the full Ship-stage dogfood run, not assume it "just works." |

## Open Questions

1. **Exact shape of the Ship-stage `ReviewFailed`/`AgentFailed` signal**
   - What we know: CONTEXT.md names both states explicitly as required handling; `AgentResult`/`AgentStatus` already has `Success`/`Failed`/`RateLimited`/`Unknown`.
   - What's unclear: Whether `ReviewFailed` becomes a new `AgentStatus` enum variant (breaking change to a `#[serde(rename_all = "lowercase")]` enum with existing test coverage) or is conveyed via the existing `reason: Option<String>` field with a string convention devflow parses (e.g. `reason` containing `"review:"` prefix), analogous to how `GateAction::from_response` already string-matches on `note.contains("abort")`.
   - Recommendation: Prefer extending `reason`-string convention (lower blast radius, no serde-format break) unless the planner has a strong reason to add a new enum variant — but this should be an explicit planning decision, not silently assumed.

2. **Should the Ship-stage prompt explicitly instruct the agent to run `/gsd-code-review` before `/gsd-ship`, or rely on `/gsd-ship`'s own optional-review step?**
   - What we know: `/gsd-ship`'s own `optional_review` step is interactive (`AskUserQuestion`) unless `workflow.code_review_command` is configured; `/gsd-code-review` is a separate, non-interactive, fully automatable skill that produces a `REVIEW.md` artifact with severity-classified findings.
   - What's unclear: Whether 13a's "13a — `/gsd-ship` + `/gsd-code-review` integration" language means devflow's `prompt::stage_prompt(Stage::Ship, ...)` should be extended to explicitly sequence `/gsd-code-review {N}` then `/gsd-ship {N}` (making review non-interactive and headless-safe), or whether it means something narrower (e.g. just correctly parsing whatever the agent reports).
   - Recommendation: Given the headless/unattended-dogfood goal (13e) and Pitfall 2's identified risk, the safer design is to have DevFlow's own prompt explicitly sequence `/gsd-code-review {N}` (non-interactive) before `/gsd-ship {N}`, and have the agent report `ReviewFailed` if `REVIEW.md` contains Critical findings — this avoids ever hitting `/gsd-ship`'s interactive `AskUserQuestion` step in headless mode at all. Flag this for explicit confirmation during planning/discuss-phase, since it changes `prompt.rs` behavior beyond pure parsing.

3. **Does 13d's worktree-by-default change interact with `Parallel`/`Sequentagent`, which already force worktree mode unconditionally?**
   - What we know: `parallel()` already calls `start(..., true, false)` (worktree hardcoded `true`) and `sequentagent` creates its own worktrees directly, bypassing `Start`'s flag entirely.
   - What's unclear: Whether flipping `Start`'s default has any observable effect on these two commands (it shouldn't, since they don't read the CLI flag's default — they call `start()` as a function with an explicit bool) — low risk, but worth a planner test asserting `parallel`/`sequentagent` behavior is unchanged by the flip.
   - Recommendation: Add a regression test (or confirm existing `parallel_creates_two_worktrees_and_spawns_two_monitors` / `sequentagent_*` tests already cover this) asserting no behavior change post-flip.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | All git-flow/worktree operations | ✓ | (repo is a live git checkout; git operations already exercised throughout existing test suite) | — |
| `cargo`/`rustc` | Build/test the workspace | ✓ | Workspace edition 2024, builds today (existing CI) | — |
| `gh` CLI | Ship-stage `/gsd-ship` workflow (external, invoked by the agent, not by devflow's Rust code) | Not verified from devflow's own code path — this is the *agent's* runtime dependency, not devflow's. 12-12-SUMMARY.md's manual verification already confirmed `devflow doctor` correctly detects `gh` presence. | — | If `gh` is missing/unauthenticated when the Ship-stage agent runs `/gsd-ship`, that workflow already has its own preflight check (`which gh && gh auth status`) and exits with setup instructions — this surfaces as an `AgentFailed`-shaped `DEVFLOW_RESULT: failed` from devflow's perspective (13a's new failure path must handle it). |
| `claude` CLI | Claude adapter (13e dogfood run, Claude leg) | ✓ (per `devflow doctor`, confirmed live in 12-12-SUMMARY.md) | Not pinned in this repo; real invocation in 12-12 used a live paid API call | — |
| `codex` CLI | Codex adapter (13e dogfood run, Codex leg; also 13b's Codex JSONL parser needs a real invocation to confirm the exact event shapes against the installed CLI version) | ✓ per `devflow doctor`'s existing check (not independently re-verified in this research session — no live Codex invocation was made) | Unconfirmed exact installed version | 13e's dogfood run is itself the verification step; if the installed Codex CLI's `--json` output differs from the publicly documented schema (Pitfall 5's concern extends to Codex too), the planner should budget time to adjust the parser against real captured output, exactly as 12-12-SUMMARY.md did for Claude. |

**Missing dependencies with no fallback:** None identified — all required tooling is either already confirmed present (per `12-12-SUMMARY.md`'s `devflow doctor` run) or is the external agent's own runtime concern, not a blocking dependency of devflow's Rust code.

**Missing dependencies with fallback:** `gh` auth/availability at Ship-stage runtime is handled by `/gsd-ship`'s own preflight (external), surfacing to devflow as a parseable agent failure once 13a's Ship-failure branch exists.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Cargo's built-in test harness (`#[test]`), no external test framework — matches every existing module in this workspace |
| Config file | none — plain `cargo test` |
| Quick run command | `cargo test -p devflow-core` / `cargo test -p devflow-cli` (scoped to the crate under active work) |
| Full suite command | `cargo test` (workspace-wide; matches `.github/workflows/ci.yml`'s `cargo test` step) [VERIFIED: .github/workflows/ci.yml] |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| 13a | `handle_ship_outcome` success path unaffected by new failure branch | unit | `cargo test -p devflow-cli advance_ship_success_runs_finish_workflow -- --exact` | ✅ existing (`crates/devflow-cli/src/main.rs` `#[cfg(test)]` mod) |
| 13a | Ship-stage `AgentFailed` fires a gate + notify hook (never silent) | unit (new) | `cargo test -p devflow-cli ship_agent_failed_fires_gate -- --exact` (name illustrative) | ❌ Wave 0 — new test needed |
| 13a | Ship-stage `ReviewFailed` loops back to Code | unit (new) | `cargo test -p devflow-cli ship_review_failed_loops_to_code -- --exact` | ❌ Wave 0 — new test needed |
| 13a | Dead `ship.rs` code (`LastShip`, `build_pr_body`, etc.) removed without breaking `prepend_changelog`/cron-instructions callers | unit (existing, must still pass after deletion) | `cargo test -p devflow-core ship::tests` (keep only the tests for surviving functions) | ✅ existing, subset to prune |
| 13a | Full-Ship manual verification re-run (was BLOCKED in 12-12) | manual (`checkpoint:human-verify`) | N/A — human-in-the-loop, real `gh`/agent invocation as `12-12-SUMMARY.md` Task 1/2 did | manual only |
| 13b | Validate-with-gaps does not advance to Ship (verdict-vs-ran split) | unit (new) | `cargo test -p devflow-core validate_gaps_does_not_advance -- --exact` (illustrative) | ❌ Wave 0 — new test needed |
| 13b | Zero-commit Define/Validate runs don't trip Layer 2 | unit (new, extends existing `evaluate_layer2_*` tests) | `cargo test -p devflow-core evaluate_layer2` | 🟡 partial — existing tests cover Code-stage zero-commit; a stage-scoping test is new |
| 13b | Claude envelope `is_error: true` detected as failure even with no `DEVFLOW_RESULT` marker | unit (new) | `cargo test -p devflow-core claude_envelope_is_error_detected -- --exact` (illustrative) | ❌ Wave 0 — new test needed |
| 13b | Codex JSONL `turn.failed` event parsed as failure; `turn.completed` as success | unit (new) | `cargo test -p devflow-core codex_event_stream_parsing` (illustrative) | ❌ Wave 0 — new module/tests needed |
| 13c | Non-Validate stage failure (WR-11) fires gate + notify hook | unit (new) | `cargo test -p devflow-cli non_validate_failure_fires_gate_and_hook -- --exact` (illustrative) | ❌ Wave 0 — new test needed |
| 13c | Notify hook failure is fail-soft (never blocks the loop) | unit (new) | `cargo test -p devflow-core notify_hook_failure_is_fail_soft -- --exact` (illustrative) | ❌ Wave 0 — new test needed |
| 13c | `GATE_TIMEOUT_SECS` configurable via env var, defaults to 7 days when unset/unparseable | unit (new) | `cargo test -p devflow-cli gate_timeout_env_override` (illustrative) | ❌ Wave 0 — new test needed |
| 13d | `devflow start` defaults to worktree; opt-out flag works; `parallel`/`sequentagent` unaffected | integration (existing pattern) | `cargo test -p devflow --test phase7_cli` (extend `reference_and_cleanup_worktree_cli_flow`-style test to assert default-without-`--worktree`) | 🟡 partial — existing fake-bin integration harness covers the mechanics; a specific "default is worktree" assertion is new |
| 13e | Full dogfood run (Claude, full loop) | manual (`checkpoint:human-verify`) | N/A — real external project, real credentials, real gate/notify hook | manual only |
| 13e | Codex leg (Code→Validate minimum) | manual (`checkpoint:human-verify`) | N/A | manual only |

### Sampling Rate
- **Per task commit:** `cargo test -p devflow-core` or `-p devflow-cli` scoped to whichever crate the task touches
- **Per wave merge:** `cargo test` (full workspace) + `cargo clippy -- -D warnings` + `cargo fmt --check` (all three already gate CI per `.github/workflows/ci.yml`)
- **Phase gate:** Full suite green, plus the 13a Full-Ship re-verification and 13e dogfood run(s) recorded as PASS before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] New unit tests for Ship-stage `AgentFailed`/`ReviewFailed` branches in `crates/devflow-cli/src/main.rs` (13a)
- [ ] New unit tests for stage-scoped Layer-2 gating and `is_error`/`num_turns` envelope reads in `crates/devflow-core/src/agent_result.rs` (13b)
- [ ] New Codex JSONL event-stream parser + its unit tests in `crates/devflow-core/src/agent_result.rs` (or a new small module) (13b)
- [ ] New unit tests for WR-11's gate+notify fix and the notify-hook's fail-soft contract (13c)
- [ ] New/extended integration test in `crates/devflow-cli/tests/phase7_cli.rs` asserting worktree-by-default + opt-out flag (13d)
- [ ] No new test framework or config needed — Cargo's built-in harness already covers every existing test in this workspace

*(No shared fixtures beyond the `init_repo`/`fake_bin_dir`/`git()` helpers already duplicated across `main.rs`, `agent_result.rs`, `hooks.rs`, `worktree.rs`, `phase7_cli.rs`, and `monitor_e2e.rs` — the planner may consider whether consolidating these into a shared `tests/common.rs` is in scope, but note CLAUDE.md's "surgical changes" preference argues against an unrequested refactor here.)*

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | DevFlow is a single-operator local CLI; no auth surface of its own (agent CLIs handle their own API-key auth externally) |
| V3 Session Management | No | No session concept beyond `.devflow/state.json`, already covered under V4/V5 below |
| V4 Access Control | No | Single-operator local tool; filesystem permissions are the only boundary, unchanged by this phase |
| V5 Input Validation | Yes | The new notify-hook command and any gate-context text passed to it must not allow shell injection; the existing `shell_quote`/argv-literal patterns (`monitor.rs::shell_escape`, `main.rs`'s WR-01 argv-not-shell-interpolation fix) must be followed for any new shell-out |
| V6 Cryptography | No | No cryptographic operations in this phase |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Shell injection via the new notify-hook command if gate context (phase/stage/human-readable text) is interpolated into a shell string unescaped | Tampering | Follow the WR-01 precedent already fixed in this codebase (`monitor.rs::shell_escape`, argv-based `Command::new(program).args(&args)` rather than string-interpolated `sh -c "$cmd $context"`). If the notify command needs the gate context, pass it via an environment variable or piped stdin to the user-supplied command rather than string-concatenating it into the command line. |
| A malicious/corrupted `.devflow/gates/NN-{stage}.response.json` (local trust boundary — same actor who can write this file already has full filesystem access to the project) | Tampering | Out of scope to harden further in this phase — the existing gate protocol already assumes local filesystem trust (documented in `gates.rs`'s module doc); no new attack surface is introduced by 13c's notify hook (it *reads* gate state to fire, it does not consume untrusted external input to construct the command beyond what the operator configures) |
| `--dangerously-skip-permissions` running an agent with full tool access outside the worktree boundary if 13d's default flip has a bug that leaves an agent running in the main checkout unintentionally | Elevation of Privilege | This is precisely why 13d exists — worktree-by-default is the mitigation. The planner should add a test explicitly asserting the *default* (no flags) path creates a worktree, not just that `--worktree`/`--no-worktree` flags individually work as expected. |

## Sources

### Primary (HIGH confidence)
- Direct source reads of this repository at the current `develop` HEAD: `crates/devflow-cli/src/main.rs`, `crates/devflow-core/src/{ship,gates,worktree,recover,agent,agent_result,agents/{claude,codex,mod},stage,mode,prompt,state,hooks,workflow,config}.rs`, `crates/devflow-cli/tests/phase7_cli.rs`, `crates/devflow-core/tests/monitor_e2e.rs`, `.github/workflows/ci.yml`, `Cargo.toml`/`Cargo.lock` (all workspace crate versions), `.planning/phases/11-refactor-gsd-native/11-VALIDATION.md`, `.planning/phases/12-bootstrap-housekeeping/12-12-SUMMARY.md`, `.planning/STATE.md`, `.planning/ROADMAP.md`, `.planning/phases/13-mvp-core-loop/CONTEXT.md`
- `~/.claude/skills/gsd-ship/SKILL.md` and `~/.claude/gsd-core/workflows/ship.md` (external GSD workflow definition — confirms `/gsd-ship`'s actual push/PR/review mechanics, read directly from the local install)
- `~/.claude/skills/gsd-code-review/SKILL.md` (external GSD workflow definition)

### Secondary (MEDIUM confidence — official docs, cross-referenced)
- [Claude Code CLI reference](https://code.claude.com/docs/en/cli-reference) — confirms `--output-format` accepts `text`/`json`/`stream-json`
- [Claude Code structured outputs](https://code.claude.com/docs/en/agent-sdk/structured-outputs) — confirms `ResultMessage`/`subtype` shape for the schema-constrained case (adjacent to, not identical to, plain `--output-format json`)
- [OpenAI Codex non-interactive mode docs](https://learn.chatgpt.com/docs/non-interactive-mode) (redirected from developers.openai.com/codex/noninteractive) — confirms Codex `--json` event types: `thread.started`, `turn.started`, `item.started`/`item.completed`, `turn.completed` (with `usage`), `turn.failed`
- [OpenAI Codex GitHub docs/exec.md](https://github.com/openai/codex/blob/main/docs/exec.md)

### Tertiary (LOW confidence — community/secondary sources, cross-referenced for corroboration only)
- WebSearch aggregation citing the full Claude JSON envelope example (`is_error`, `total_cost_usd`, `duration_ms`, `duration_api_ms`, `num_turns`, `session_id`) — corroborated across multiple independent third-party CLI-reference writeups, but not found verbatim in a single first-party Anthropic page during this research session
- [Anthropic GitHub issue #24596](https://github.com/anthropics/claude-code/issues/24596) "CLI `--output-format stream-json` lacks event type reference" and [#24612](https://github.com/anthropics/claude-code/issues/24612) "Document all message types emitted by `claude -p --output-format stream-json`" — both open issues confirming the documentation gap named in Pitfall 5 (these are about `stream-json` specifically, but corroborate that Anthropic's own docs for the `-p`/print-mode JSON formats are incomplete)
- A third-party "Codex exec --json event cheatsheet" (takopi.dev) — used only to corroborate event-type names already confirmed via the official redirect target above, not as a standalone source

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; all versions read directly from `Cargo.lock`
- Architecture (ship.rs reality, WR-11 location, gate/hook patterns): HIGH — every claim traced to a specific file/line read this session
- Claude/Codex envelope schemas: MEDIUM — core fields (`is_error`, `result`, `num_turns`, Codex event type names) corroborated across multiple sources including one first-party redirect target, but the *complete* enumeration of error subtypes is an acknowledged upstream documentation gap (see Sources/Pitfall 5) — treat exact non-success subtype matching as `[ASSUMED]`
- Pitfalls: HIGH for codebase-internal pitfalls (directly observed); MEDIUM for the `/gsd-ship` interactive-review-in-headless-mode risk (inferred from reading `ship.md`, not empirically reproduced this session)

**Research date:** 2026-07-14
**Valid until:** 30 days for the internal codebase findings (stable, slow-moving); 7-14 days for the external Claude/Codex CLI JSON schema claims (both vendors are actively iterating their CLI output formats and documentation — re-verify against a real captured invocation before relying on subtype-level detail, exactly as `12-12-SUMMARY.md` already demonstrated is necessary practice for this codebase)
