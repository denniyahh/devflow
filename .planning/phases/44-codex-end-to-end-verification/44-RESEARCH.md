# Phase 44: Codex End-to-End Verification - Research

**Researched:** 2026-08-26
**Domain:** Rust CLI pipeline (DevFlow) — modular agent driver dogfood, CLI resume/handoff surface, Hermes cron scheduling integration
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Codex E2E Dogfood (CODE-01)**
- **D-01: Phase 44 is both verification and hardening vehicle.** The Codex run is not a separate
  ceremonial acceptance pass; it should exercise the hardening surface in #147/#148/#153 where
  practical, then record which gaps were closed and which were re-filed.
- **D-02: Codex must not be asked to run interactive Define/Plan prompts.** Codex's driver declares
  `Define` and `Plan` as `RequiresExistingArtifact`; this context exists so downstream planning can
  create executable artifacts before any live Codex E2E run expects headless progress.
- **D-03: Success evidence must distinguish "completed a phase" from "surfaced a gap."** A real
  Codex run may satisfy CODE-01 by completing or by producing concrete re-filed gaps, but the final
  verification must say which happened and must not treat local green tests alone as Codex E2E
  proof.
- **D-04: Driver parity guard is required.** Any handoff/resume changes must preserve current Codex
  launch invariants: `codex -a never exec --sandbox workspace-write --json`, writable roots for
  linked-worktree git metadata, scoped signing disable env, Codex-native workflow prompt rendering,
  and existing conformance tests.

**Agent Handoff via Resume (#147 / DEN-60)**
- **D-05: Implement handoff as `devflow resume --phase N --agent <AGENT>`.** This is the primary
  surface because it preserves the existing "continue the saved stage" contract. A separate
  `devflow handoff` command is deferred unless planning finds the resume flag creates an
  unresolvable CLI ambiguity.
- **D-06: Handoff mutates only the saved agent, not the run identity.** Preserve stage, mode,
  worktree path, branch, gates, verification baseline/fingerprint, `stop_until`, `yes_ship`,
  failure counters, checkpoint counters, and any other persisted state not directly about the
  selected driver.
- **D-07: Persist the new agent before monitor relaunch and emit an audit event.** The state file
  must show the new agent before the detached monitor can read it. `.devflow/events.jsonl` gets a
  handoff event with phase, from-agent, to-agent, stage, and reason/source (`resume --agent`).
- **D-08: Refuse unsafe handoffs with the same preflight seriousness as start.** Handoff to a driver
  that cannot run the current saved stage headlessly must fail before state mutation. Codex at
  `Define`/`Plan` is the key negative case unless the required artifact already exists and the
  existing `RequiresExistingArtifact` rule allows continuation.
- **D-09: Rate-limited state is an allowed handoff source.** A parked/rate-limited run is the
  motivating case; `resume --agent codex` must be able to replace the current agent when the saved
  stage can safely be relaunched under Codex.

**Hermes Cron Resume Contract (#148)**
- **D-10: Remove the printed `hermes cron create --from-devflow` instruction.** The installed Hermes
  CLI does not support that flag, and current source still prints it in `cron_hint_line`; leaving it
  would keep the operator-facing instruction unusable.
- **D-11: Use the existing Hermes cron surface, not a speculative hidden intake flag.** Emit a
  command that can be built from current documented flags, preferably a one-shot `--script` /
  `--no-agent` style watchdog that runs the persisted `devflow resume --phase N` command from the
  cron record. If planning discovers a stable Hermes contract has landed, it may use that only after
  verifying the installed CLI help exposes it.
- **D-12: Generate schedules in the timezone Hermes will interpret, or avoid cron-field ambiguity.**
  Current `ship.rs` normalizes retry timestamps to UTC cron fields; that is wrong when Hermes cron
  interprets `M H D M W` in America/New_York/local scheduler time. The planned fix must either
  convert absolute retry time into the scheduler's local time before emitting cron fields or use a
  timezone-aware Hermes mechanism if one exists and is verified.
- **D-13: Keep unparseable retry times fail-closed.** Existing behavior that avoids turning bad
  retry text into `* * * * *` is correct and must be retained.
- **D-14: Validate both sides of the cron hint.** Tests need a positive local-time conversion case
  and a negative control showing the old UTC-field schedule would fire at the wrong local instant.
  The CLI hint test must assert the unsupported `--from-devflow` string is absent.

**Cron Record Consumption (#153)**
- **D-15: Delete cron-instruction records only after genuine consumption.** Consumption means a
  resume relaunch actually started, or the phase completed/ship cleanup reached a terminal success
  point. Invoking `devflow resume` and failing before monitor launch must keep the record.
- **D-16: Tie resume-side deletion to confirmed launch evidence.** The safest local signal is after
  `launch_stage` succeeds and the state/monitor pid has been written. Do not delete before the
  relaunch is durable.
- **D-17: Ship-side cleanup is belt-and-braces.** Delete any remaining phase cron record on
  successful ship/completion, using the existing idempotent `delete_cron_instructions`.
- **D-18: Emit an audit event for deletion.** Record phase, path kind (`per-phase` vs legacy if
  knowable), and trigger (`resume_consumed` or `ship_complete`) in `.devflow/events.jsonl`.
- **D-19: Do not change recover/clean semantics for unconsumed records.** Existing `recover::clean`
  deletion remains a reset/cleanup path, not the consumption definition.

### Claude's Discretion
- Exact event field names for handoff and cron deletion, provided they are documented in tests and
  consistent with existing event naming style.
- Whether `resume --agent <same-agent>` is accepted as an idempotent resume or rejected as a no-op.
- Exact helper boundaries for local-time conversion and Hermes command rendering.

### Deferred Ideas (OUT OF SCOPE)
- A separate `devflow handoff --phase N --to <AGENT>` command is deferred unless `resume --agent`
  proves structurally unsuitable.
- General scheduler abstraction beyond Hermes is out of scope.
- Broad cleanup of historical Codex research/docs is out of scope except where needed for CODE-01
  evidence.
- New agent drivers, extra dashboard/status UI, and generic event-schema redesign are out of scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CODE-01 | `--agent codex` verified end-to-end through a real phase (dogfood); surfaced gaps closed or re-filed | Codex driver contract fully re-verified against source this session (launch argv, env, interactivity — see Code Examples); dogfood evidence bar and capture-directory pattern documented under Validation Architecture / Wave 0 Gaps; #147/#148/#153 hardening researched below as the surface the dogfood run is expected to exercise per CONTEXT.md D-01 |

</phase_requirements>

## Summary

Phase 44 has two halves that share one code path. Half one (CODE-01) is a **dogfood run**, not new
code: `--agent codex` already exists as a fully modular driver
(`crates/devflow-core/src/agents/codex.rs`) with its own conformance tests, writable-roots
handling, and signing-disable environment — it has never been driven through a full, real
Define→Ship phase and the run itself is the deliverable. Half two is **three small, well-isolated
hardening fixes** (#147/#148/#153) layered onto the exact machinery the dogfood run will exercise
— agent handoff via `devflow resume --agent`, a Hermes cron-hint rewrite, and cron-record
deletion on consumption. All three fixes touch code that already has direct call sites read this
session; none require a new architectural pattern.

The single most consequential finding is about #148: the CONTEXT.md's proposed direction —
converting the UTC retry timestamp into "the scheduler's local time" before emitting `M H D M W`
cron fields — is avoidable entirely. Reading Hermes's own installed source
(`~/Github/hermes-agent/cron/jobs.py:732-829`) shows `hermes cron create` accepts a full ISO-8601
timestamp **with an explicit UTC offset** (`2026-06-18T15:45:30Z`) as a `schedule` argument, and
when the offset is present Hermes treats it as tz-aware and never reinterprets it against the
locally configured zone (`dt.tzinfo is None` gate at `jobs.py:800`). DevFlow already computes a
UTC-normalized `RetryTimestamp` internally (`ship.rs`); it just needs to render that as an
ISO-8601-with-`Z` schedule string instead of bare cron fields. This requires **no new timezone
crate dependency** — DevFlow currently has zero timezone-aware dependencies in its `Cargo.toml`
graph, and reaching for one (e.g. `chrono-tz`) to do the UTC→local conversion CONTEXT.md's D-12
describes would be over-engineering a problem that has a zero-dependency fix.

**Primary recommendation:** Treat CODE-01 as a real, single supervised Codex run through this
worktree's own phase-planning pipeline (the standard "dogfood the tool on itself" pattern this
project has used since Phase 13); implement #147 as a state-mutation-then-relaunch inside the
existing `resume()` function; implement #148 by switching `HermesCronJob.schedule` from `M H D M W`
cron fields to an offset-qualified ISO-8601 timestamp and replacing `cron_hint_line`'s literal
`--from-devflow` string with a real `hermes cron create <iso-timestamp> "<command>" --repeat 1
--name <job>` invocation built from flags confirmed present in the installed CLI; implement #153 by
adding `delete_cron_instructions` calls at the two points already read this session where
consumption is provably genuine — `spawn_agent_and_record`'s successful pid write (resume-side) and
`finish_workflow_with_gate_timeout`'s `workflow::clear_state` call (ship-side).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Codex launch argv / prompt rendering | `devflow-core` (agent driver) | — | `AgentDriver` trait owns per-agent launch shape by design (999.31); CLI never branches on `AgentKind` for launch mechanics |
| Agent handoff (`resume --agent`) | `devflow-cli` (pipeline_launch) | `devflow-core` (state persistence) | State mutation + relaunch orchestration is CLI-pipeline-owned (existing `resume()`); `State`/`AgentKind` fields it touches live in `devflow-core` |
| Interactivity/headless-safety gating | `devflow-core` (`AgentDriver::interactivity_mode`) | `devflow-cli` (`preflight.rs` consumes it) | Driver declares the contract; CLI preflight enforces it — established split, do not invert |
| Hermes cron-instructions schema + schedule math | `devflow-core` (`ship.rs`) | — | Pure data/transform module, no process I/O; CLI only prints/reads it |
| Cron-record consumption/deletion | `devflow-cli` (pipeline_launch resume, pipeline_gate finish_workflow) | `devflow-core` (`ship::delete_cron_instructions`, already idempotent) | Deletion *trigger* is a CLI orchestration decision (when is consumption "genuine"); the deletion *primitive* is core |
| Rate-limit → cron-record write | `devflow-cli` (`pipeline_outcomes::handle_rate_limited_outcome`) | `devflow-core` (`ship::build_single_agent_cron_instructions`) | Unchanged by this phase; listed for completeness since #148/#153 both touch its output |

## Standard Stack

This phase adds no new external dependency to either crate. It modifies existing, already-adopted
internals only.

### Core
No new libraries. The relevant existing internal modules:

| Module | Purpose | Why it's the right layer |
|--------|---------|---------------------------|
| `devflow_core::agents::codex::CodexDriver` | Codex launch argv, environment, interactivity | Already the sole owner of Codex-specific behavior (999.31 migration complete since Phase 37) `[VERIFIED: crates/devflow-core/src/agents/codex.rs:1-101]` |
| `devflow_core::ship` | `CronInstructions`/`HermesCronJob` schema, schedule math, idempotent delete | Already owns every cron-record read/write/delete primitive `[VERIFIED: crates/devflow-core/src/ship.rs:1-154]` |
| `devflow_cli::pipeline_launch::resume` | Loads saved state, relaunches saved stage | Existing resume entry point; #147 extends it rather than adding a parallel path `[VERIFIED: crates/devflow-cli/src/pipeline_launch.rs:1237-1286]` |

### Supporting
None required.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| ISO-8601-with-offset Hermes schedule (recommended fix for #148) | `chrono-tz` + operator-configured IANA zone, converting UTC→local before emitting `M H D M W` fields | Adds a new Cargo dependency, a Package Legitimacy Audit, and DST-correctness surface area DevFlow does not otherwise need — CONTEXT.md's D-12 phrasing ("convert absolute retry time into the scheduler's local time") assumed this path; the ISO-timestamp path is functionally equivalent and zero-dependency `[VERIFIED: ~/Github/hermes-agent/cron/jobs.py:783-809, this session]` |
| A separate `devflow handoff` command (#147) | `resume --agent <AGENT>` | CONTEXT.md D-05 already decided this; `resume --agent` reuses 100% of the existing save/relaunch ordering, a new command would duplicate it |

**Installation:** N/A — no new packages.

## Package Legitimacy Audit

**Not applicable.** This phase installs no external packages in either the Rust workspace or any
other ecosystem. All work modifies existing first-party modules (`ship.rs`, `pipeline_launch.rs`,
`pipeline_outcomes.rs`, `commands.rs`, `recover.rs`, `main.rs`) already present in the repository.
If a planner is tempted to add a timezone crate for #148, see "Alternatives Considered" above —
the recommended fix needs none.

## Architecture Patterns

### System Architecture Diagram — rate-limit → cron → resume → consumption lifecycle

```
 [advance() monitor loop, any agent]
        |
        v
  agent result: RateLimited  ---------------------------------+
        |                                                      |
        v                                                      |
 handle_rate_limited_outcome (pipeline_outcomes.rs)             |
        |  build_single_agent_cron_instructions (ship.rs)       |
        |  write_cron_instructions -> .devflow/cron-              |
        |    instructions-{NN}.json                              |
        v                                                      |
 [operator runs `devflow status` -> cron_hint_line (commands.rs)]  <- #148 fixes THIS text
        |
        v
 operator (or Hermes cron firing the printed command)
 runs: devflow resume --phase N [--agent <AGENT>]   <- #147 adds [--agent]
        |
        v
 pipeline_launch::resume()
   1. acquire per-phase lock
   2. load_state
   3. [NEW, #147] if --agent given:
        a. pre-check interactivity_mode(new_agent, state.stage)
           BEFORE mutating state (D-08) — see Pitfall 1
        b. state.agent = new_agent
        c. save_state (agent visible before relaunch, D-07)
        d. emit "handoff" event
   4. clear stopped/stop_until if fired
   5. apply_legacy_launch_opt_out (unchanged)
   6. repair_leaked_auto_chain_flag (unchanged)
   7. save_state
   8. launch_stage -> ... -> spawn_agent_and_record
        |
        v
      monitor_pid written + workflow::save_state   <- #153 resume-side delete point
        |
        v
      [NEW, #153] delete_cron_instructions(phase)  <- only AFTER pid persisted (D-16)
        |
        v
 ... stage progresses through Code/Validate/Ship ...
        |
        v
 finish_workflow_with_gate_timeout (pipeline_gate.rs:275)
   workflow::clear_state(...)
   [NEW, #153] delete_cron_instructions(phase)     <- ship-side belt-and-braces (D-17)
   events::emit("workflow_shipped" / "workflow_finished")
```

### Recommended Project Structure
No new files. Changes land in:
```
crates/devflow-cli/src/
├── main.rs              # Resume subcommand: add `--agent: Option<AgentKind>`
├── pipeline_launch.rs   # resume(): handoff mutation + pre-check + relaunch; spawn_agent_and_record: resume-side delete
├── pipeline_gate.rs     # finish_workflow_with_gate_timeout: ship-side delete
├── pipeline_outcomes.rs # (read-only reference point; retry_after computation unchanged)
├── commands.rs          # cron_hint_line: rewrite to real hermes cron create invocation
crates/devflow-core/src/
├── ship.rs              # HermesCronJob.schedule: switch to ISO-8601-with-offset; add a rendering helper
```

### Pattern 1: Pre-mutation safety check before a handoff (D-08)
**What:** Validate the target agent CAN run the currently-saved stage headlessly *before* writing
`state.agent`, not after.
**When to use:** Any `resume --agent` call.
**Why it must be a pre-check, not a reuse of `run_preflight`:** `run_preflight`'s
`preflight_interactivity_check` (`preflight.rs:607-634`) only refuses `RequiresExistingArtifact` at
`Stage::Define` — it deliberately does NOT gate `Stage::Plan` even though `CodexDriver` declares
`Stage::Plan` as `RequiresExistingArtifact` too (the code comment explains why: "Plan is
deliberately un-gated because PLAN.md is an *output* the phase itself produces"). This is the
project's own **existing, intentional** narrowing — it is not a #147 defect to fix, but the D-08
handoff refusal check must reuse (or mirror) exactly this predicate, not a stricter one, or a
legitimate `resume --agent codex` at `Stage::Plan` would be wrongly refused.
```rust
// Source: crates/devflow-cli/src/preflight.rs:607-634 (existing function, read this session)
fn preflight_interactivity_check(project_root: &Path, state: &State) -> Result<(), String> {
    use devflow_core::agents::InteractivityMode;
    let driver = agents::driver_for(state.agent);
    match driver.interactivity_mode(state.stage) {
        InteractivityMode::HeadlessSafe => Ok(()),
        InteractivityMode::RequiresExistingArtifact => {
            if state.mode == Mode::Auto
                && state.stage == Stage::Define
                && !phase_artifact_on_develop(project_root, state.phase, "-CONTEXT.md")
            {
                return Err(format!(/* ... */));
            }
            Ok(())
        }
        other => Err(format!(/* ... */)),
    }
}
```
The recommended shape for #147's pre-check: extract a variant of this function (or call it with a
*hypothetical* `state` whose `.agent` is the requested target, constructed but not yet saved) so
the SAME rule governs both an ordinary launch's preflight and a handoff's pre-check — two separate
copies of "can this driver run this stage" is exactly the drift class this codebase's own doc
comments warn against elsewhere (`apply_legacy_launch_opt_out`'s sibling-pairing note,
`stream_launch_enabled`'s "two separate notions... free to drift" note).

### Pattern 2: State-preserving mutation (D-06)
**What:** A handoff changes `state.agent` and nothing else observable to the pipeline.
**Example — the exact field list `resume --agent` must leave untouched**, read directly from the
`State` struct definition this session (`crates/devflow-core/src/state.rs`, `pub struct State`
body):
```
stage, phase, mode, gate_pending, consecutive_failures, infra_failures,
preflight_retries, last_validate_failure_commit_count, phase_validate_failures,
last_verification_fingerprint, verification_baseline_captured,
last_verification_mtime_nanos, verification_run_nonce, started_at, project_root,
worktree_path, session_id, checkpoint_resumes, stop_until, stopped, stop_reason,
yes_ship, canary, legacy_claude_launch
```
`monitor_pid` is the one field that legitimately changes as a *side effect* of the relaunch
(`spawn_agent_and_record` always clears then rewrites it) — that is existing, unrelated behavior,
not something the handoff mutation itself should touch.

### Pattern 3: Loud, event-logged escape hatches (established house style)
**What:** Every existing "operator override" path in this codebase — `--legacy-claude-launch`
(D-11, `apply_legacy_launch_opt_out`), `--yes-ship`, the auto-chain-flag repair — follows the same
three-channel loudness contract: `println!` to the operator, a line in the phase's monitor log
(where relevant), and an `events::emit(...)` record in `.devflow/events.jsonl`. A handoff event
should follow the identical shape rather than inventing a new convention.
```rust
// Source: crates/devflow-cli/src/pipeline_launch.rs:1083-1094 (checkpoint_auto_decided, the
// closest existing precedent — an operator-visible state change recorded before it takes effect)
events::emit(
    &state.project_root,
    state.phase,
    "checkpoint_auto_decided",
    serde_json::json!({
        "stage": state.stage.to_string(),
        "session_id": session_id,
        "instruction": truncate_reason(&instruction),
        "attempt": state.checkpoint_resumes,
        "policy": "D-03: unconditional agent auto-decide, no flag/config toggle",
    }),
);
```
Field-name suggestion for the `handoff` event (Claude's Discretion per CONTEXT.md): `stage`,
`from_agent`, `to_agent`, `reason` (e.g. `"resume --agent"`), matching D-07's literal requirement.

### Anti-Patterns to Avoid
- **Re-detecting rate-limit or agent-health state inside the cron-hint renderer.** MemPalace recall
  flags this explicitly: `cron_hint_line` composition already forbids introducing
  `detect_rate_limit`/`detect_claude_rate_limit`/`detect_codex_rate_limit` (21-02 precedent,
  verified by grep returning zero hits at the time). The #148 rewrite must only *read* the already-
  persisted `CronInstructions`/`retry_after`, never re-derive rate-limit status.
- **Converting to local time via hand-rolled offset arithmetic.** `ship.rs` already hand-rolls
  civil-calendar math (`days_from_civil`/`civil_from_days`) with **no IANA timezone database** —
  there is no DST table anywhere in this dependency graph `[VERIFIED: no chrono/chrono-tz/jiff/time
  crate present in any Cargo.toml in the workspace, checked this session]`. Any "convert UTC to
  local wall-clock" implementation that does not either (a) pull in a real tz database or (b) avoid
  the problem entirely (the ISO-offset-timestamp fix, above) will be silently wrong across a DST
  transition.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| UTC retry time → Hermes schedule | A UTC→local wall-clock converter (needs a real tz database to be DST-correct) | An ISO-8601 timestamp string carrying an explicit `Z`/offset, passed straight through to `hermes cron create` | Hermes's own `parse_schedule` (`~/Github/hermes-agent/cron/jobs.py:783-809`) already treats an offset-qualified timestamp as unambiguous and skips its local-zone reinterpretation entirely — verified by reading the source this session |
| "Which stages can this driver run headless" | A second, handoff-specific interactivity table | `AgentDriver::interactivity_mode` (already exists, already the single source of truth consumed by `preflight.rs`) | A second table is exactly the kind of "two notions of the same fact, free to drift" this codebase's own comments repeatedly warn against |
| Cron-record "is this consumed yet" heuristic | Path/mtime sniffing on `.devflow/cron-instructions-{NN}.json` | The two concrete, already-identified call sites (post-`monitor_pid` write; post-`clear_state`) | Both are places where "did the run genuinely restart / genuinely finish" is already unambiguous in existing code — no new signal needs inventing |

**Key insight:** every piece of machinery #147/#148/#153 need already exists in this codebase in a
form built for a near-identical purpose (interactivity gating, loud audit events, idempotent
delete). The phase's work is wiring, not new subsystems.

## Common Pitfalls

### Pitfall 1: Gating a handoff more strictly than an ordinary launch gates itself
**What goes wrong:** A naive #147 implementation checks `interactivity_mode(new_agent, stage) ==
HeadlessSafe` and refuses everything else. That would refuse `resume --agent codex` at
`Stage::Plan` even though the codebase's own existing preflight explicitly does NOT refuse Codex at
Plan (only at Define, and only in Auto mode with a missing artifact).
**Why it happens:** `RequiresExistingArtifact` reads as "always refuse headless" if you don't read
`preflight_interactivity_check`'s actual `match` arms.
**How to avoid:** Reuse the same predicate `preflight.rs:607-634` implements (see Pattern 1 above),
applied against a state whose `.agent` is the *candidate* target, not a fresh, stricter rule.
**Warning signs:** A new regression test that expects `resume --agent codex` to succeed at
`Stage::Plan` (matching D-08's own text: "Codex at Define/Plan is the key negative case *unless the
required artifact already exists*") fails against your gate.

### Pitfall 2: Breaking the committed `--help` snapshot
**What goes wrong:** Adding `--agent: Option<AgentKind>` to `Command::Resume` in `main.rs` changes
`devflow --help` output. `crates/devflow-cli/tests/help_snapshot.rs`'s
`help_output_matches_committed_snapshot` diffs live `--help` against the committed
`crates/devflow-cli/tests/snapshots/devflow-help.txt` and fails on any drift `[VERIFIED:
crates/devflow-cli/tests/help_snapshot.rs:1-41]`.
**Why it happens:** Easy to forget — the test lives in a separate `tests/` integration-test file,
not beside `main.rs`.
**How to avoid:** After adding the flag, regenerate the snapshot as the test's own doc comment
instructs: `cargo run -q -p devflow -- --help > crates/devflow-cli/tests/snapshots/devflow-help.txt`,
then review the diff (should be exactly the new `--agent` line under `Resume`) before committing.
**Warning signs:** `cargo test help_output_matches_committed_snapshot` fails with a printed diff.

### Pitfall 3: Three existing unit tests hardcode the exact broken `--from-devflow` string
**What goes wrong:** `cron_instruction_hints_include_hermes_command_per_phase`,
`cron_hint_line_appends_sanitized_reset_when_retry_after_present`, and
`cron_hint_line_omits_reset_fragment_when_retry_after_empty` (`commands.rs:4139-4200`, all read this
session) assert the literal string `"hermes cron create --from-devflow {project}"` verbatim. Fixing
#148 without touching these tests leaves them red for the RIGHT reason (they're asserting the bug),
but D-14 requires a NEW test asserting `--from-devflow` is absent — simply deleting the old
assertions without adding the negative-control replacement fails D-14's own requirement.
**How to avoid:** Rewrite all three tests to assert the new command shape, and add one asserting
`!hint.contains("--from-devflow")`.
**Warning signs:** `cargo test cron_hint_line` / `cargo test cron_instruction_hints` failing after
the `commands.rs` change is expected and correct until these three tests are rewritten.

### Pitfall 4: `hermes cron create --script` requires the script to live under `~/.hermes/scripts/`
**What goes wrong:** CONTEXT.md D-11 suggests "a one-shot `--script` / `--no-agent` style
watchdog." The installed CLI's own `--help` text is explicit: `--script` takes "Path to a script
under `~/.hermes/scripts/`" `[VERIFIED: hermes cron create --help, run this session, installed
Hermes Agent v0.20.5]` — an arbitrary absolute path (e.g. one DevFlow writes into the project's
`.devflow/`) is not what that flag accepts. A watchdog design that assumes `--script /any/path`
works will fail at the operator's first real attempt.
**How to avoid:** Either (a) instruct the operator to install a small, generic, versioned
`~/.hermes/scripts/devflow-resume.sh` once (out of DevFlow's control after that), or (b) use the
`prompt` positional argument instead of `--script`/`--no-agent` — Hermes's LLM-driven path accepts
an arbitrary shell instruction as free text and needs no pre-installed file. Given D-11's own
fallback language ("If planning discovers a stable Hermes contract has landed, it may use that only
after verifying the installed CLI help exposes it"), and that the `--script` path requires an
out-of-band one-time operator setup step this phase cannot perform, **the `prompt`-based command is
the one buildable entirely from what's verified installed today.**
**Warning signs:** A rendered `hermes cron create` command containing `--script` pointing outside
`~/.hermes/scripts/` — this will error at Hermes's own argument-validation layer, not DevFlow's.

### Pitfall 5: `git_ls_files`-driven test/build tooling in this repo already documents several
false-green traps unrelated to this phase's new code but load-bearing for verifying it
**What goes wrong:** (Carried over from this project's own `CLAUDE.md`, restated here because
CODE-01's dogfood run and the workspace-suite regression check in Success Criterion 3 will hit
these directly.) `cargo test --exact <name>` exits 0 when the name matches nothing; `cargo test -p
devflow --lib` fails outright (the `devflow` package is binary-only, use `-p devflow --bin
devflow`); a branch's `gh run list` does not establish a PR's current-HEAD check status (use `gh pr
checks <PR>` against the current `HEAD_SHA`).
**How to avoid:** When verifying "no regression to the existing Codex driver behavior," run the
named Codex test functions (`codex_and_pi_drivers_reproduce_legacy_behavior`,
`codex_wraps_prompt_in_exec_and_json`, `codex_grants_writable_roots_for_worktree_git_metadata`,
`codex_disables_signing_via_env_others_do_not`, `codex_define_and_plan_require_an_existing_artifact`
— all read this session, `crates/devflow-core/src/agents/mod.rs`) and assert `N passed`, not just a
zero exit code.

## Code Examples

### Codex's verified, current launch contract (the parity guard D-04 requires)
```rust
// Source: crates/devflow-core/src/agents/codex.rs:26-68 (read this session)
let mut args: Vec<String> = vec![
    "-a".into(), "never".into(), "exec".into(),
    "--sandbox".into(), "workspace-write".into(),
    "--json".into(),
];
// ... writable_roots via `-c sandbox_workspace_write.writable_roots=[...]` when non-empty ...
args.push(prompt.to_string());
("codex", args)
```
Environment (signing disabled, scoped to Codex's process tree only):
```rust
// Source: crates/devflow-core/src/agents/codex.rs:78-89
vec![
    ("GIT_CONFIG_COUNT".into(), "2".into()),
    ("GIT_CONFIG_KEY_0".into(), "commit.gpgsign".into()),
    ("GIT_CONFIG_VALUE_0".into(), "false".into()),
    ("GIT_CONFIG_KEY_1".into(), "tag.gpgsign".into()),
    ("GIT_CONFIG_VALUE_1".into(), "false".into()),
]
```
Interactivity contract:
```rust
// Source: crates/devflow-core/src/agents/codex.rs:91-100
match stage {
    Stage::Define | Stage::Plan => InteractivityMode::RequiresExistingArtifact,
    _ => InteractivityMode::HeadlessSafe,
}
```
Any handoff/resume change must leave all three of these byte-identical — they are the literal
content of D-04's "driver parity guard."

### The existing per-phase cron-instructions primitives (#153 reuses these verbatim)
```rust
// Source: crates/devflow-core/src/ship.rs:139-154 (read this session)
pub fn delete_cron_instructions(project_root: &Path, phase: PhaseId) -> Result<(), ShipError> {
    let path = cron_instructions_path(project_root, phase);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let legacy = legacy_cron_instructions_path(project_root);
    if legacy.exists()
        && let Ok(contents) = std::fs::read_to_string(&legacy)
        && serde_json::from_str::<CronInstructions>(&contents)
            .map(|i| i.phase == phase)
            .unwrap_or(true)
    {
        std::fs::remove_file(&legacy)?;
    }
    Ok(())
}
```
Already idempotent (has its own `delete_cron_instructions_is_idempotent` test,
`crates/devflow-core/src/ship.rs:447-459`) — #153's call sites need no existence check before
calling it.

### The exact ship-side completion point for #153's D-17 (verified line)
```rust
// Source: crates/devflow-cli/src/pipeline_gate.rs:272-279 (read this session)
let _ = Gates::cleanup(project_root, state.phase, Stage::Validate);
let _ = Gates::cleanup(project_root, state.phase, Stage::Ship);
workflow::clear_state(project_root, state.phase)?;
// <-- delete_cron_instructions(project_root, state.phase) belongs here (D-17)
registry::deregister(project_root, state.phase);
events::emit(project_root, state.phase, "workflow_shipped", /* ... */);
```

### The exact resume-side "genuine relaunch" point for #153's D-16 (verified line)
```rust
// Source: crates/devflow-cli/src/pipeline_launch.rs:969-1035 (spawn_agent_and_record, read this
// session) — abbreviated to the load-bearing lines
state.monitor_pid = None;
workflow::save_state(state)?;
ensure_agent_binary(program)?;
/* archive_phase_files ... */
let pid = monitor::spawn_monitor(state, program, args, extra_env, launch)
    .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
state.monitor_pid = Some(pid);
workflow::save_state(state)?;
// <-- delete_cron_instructions(&state.project_root, state.phase) belongs here (D-16) —
//     AFTER the pid is durably persisted, so a spawn failure above (returned via `?`)
//     never reaches this point and the record survives for retry.
let _ = devflow_core::registry::register(&state.project_root, state.phase);
events::emit(&state.project_root, state.phase, "stage_launched", /* ... */);
```
This is `spawn_agent_and_record`, the SHARED tail used by both an ordinary stage launch and
`relaunch_checkpoint_session` — a handoff's relaunch goes through the identical path via `resume` →
`launch_stage` → `launch_stage_inner` → this function, so #147 and #153 compose without extra
plumbing: a handoff that succeeds also naturally satisfies #153's "genuine relaunch" deletion
trigger.

### Hermes's actual schedule grammar (verified against the installed CLI's source this session)
```python
# Source: ~/Github/hermes-agent/cron/jobs.py:732-829 (parse_schedule, read this session)
# Accepted forms:
#   "30m", "2h", "1d"        -> one-shot, relative duration from creation time
#   "every 30m", "every 2h"  -> recurring interval
#   "0 9 * * *"              -> cron expression (5 space-separated numeric/`*`/`-`/`,`/`/` fields)
#   "2026-02-03T14:00:00"    -> ISO timestamp; if it has NO tzinfo, Hermes stamps it with the
#                                CONFIGURED Hermes timezone (jobs.py:791-802) — ambiguous, avoid
#   "2026-02-03T14:00:00Z"   -> ISO timestamp WITH an explicit offset; Hermes keeps it as-is,
#                                UNAMBIGUOUS regardless of the scheduler's configured timezone
```
The last form is the one DevFlow should emit. `RetryTimestamp` in `ship.rs` already carries
year/month/day/hour/minute (rounded to whole-minute, second always 0 after rounding) in UTC — it
just needs a `to_iso_utc()` rendering (`format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:00Z", ...)`)
instead of `to_cron()`.

## Runtime State Inventory

Not applicable — this is a feature-addition/hardening phase (new CLI flag, schedule-format change,
new deletion call sites), not a rename/refactor/migration. No stored data, live service
configuration, OS-registered state, or build artifact carries a name being changed by this phase.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Field-name suggestions for the `handoff` and cron-deletion events (`from_agent`/`to_agent`/`reason`, `trigger`) | Pattern 3 | Low — CONTEXT.md explicitly marks exact field names as Claude's Discretion; any consistent, tested naming satisfies D-07/D-18 |
| A2 | `--repeat 1` on `hermes cron create` reproduces the current `HermesCronJob.once: bool` semantics (job auto-removed after one run) | Code Examples (Hermes schedule grammar) | Medium — verified `repeat: How many times to run (None = forever, 1 = once)` from `cron/jobs.py:1945`'s docstring `[VERIFIED: ~/Github/hermes-agent/cron/jobs.py, this session]`, but the full `create_job` removal-on-completion behavior for `repeat=1` was not traced end-to-end; if wrong, the created job would need a manual `hermes cron remove` after firing instead of auto-cleaning itself — cosmetic, not correctness-affecting for #148/#153's actual DevFlow-side work |
| A3 | The recommended `prompt`-based (not `--script`/`--no-agent`) Hermes command form for #148 is the right tradeoff given D-11's constraints | Pitfall 4 | Medium — this is the strongest evidence-backed reading of D-11 available without operator input; if the operator strongly prefers the zero-LLM-cost watchdog path, they will need to pre-install a script under `~/.hermes/scripts/` themselves, which is outside this phase's control either way |

## Open Questions (RESOLVED)

CONTEXT.md's own `<open_questions>` section states: "None before planning... Planner may surface
operator questions only if installed Hermes exposes multiple verified, incompatible cron contracts
that would change user-facing behavior." This research found exactly one place where that
condition could apply: the `--script`-vs-`prompt` choice in Pitfall 4. It is resolved above with a
recommendation (use `prompt`) rather than escalated, because both paths are verified-present in the
installed CLI (not incompatible/unverified) — the tradeoff is cost/setup-burden, not existence, so
it stays inside "Claude's Discretion... exact helper boundaries for... Hermes command rendering"
per CONTEXT.md.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `codex` CLI | CODE-01 real dogfood run | ✓ | codex-cli 0.147.0 `[VERIFIED, this session]` | — |
| `hermes` CLI | #148 fix verification (schedule/flag confirmation) | ✓ | Hermes Agent v0.20.5 (2026.8.19) `[VERIFIED, this session]` | — |
| `gh` CLI | Ship-stage credential preflight (existing, unrelated to this phase's own tests) | ✓ | authenticated (used to fetch #147/#148/#153 this session) | — |
| Rust toolchain / `cargo` | All plans | ✓ (implied by existing green workspace; not re-run this session per CLAUDE.md's "do not run cargo build/test during discussion" carried into research scope) | — | — |

**Missing dependencies with no fallback:** None.
**Missing dependencies with fallback:** None.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` (no external test framework) |
| Config file | none — workspace `Cargo.toml` at repo root |
| Quick run command | `cargo test -p devflow --bin devflow <filter>` / `cargo test -p devflow-core --lib <filter>` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CODE-01 (dogfood evidence) | A real Codex-driven phase reaches Ship or surfaces a re-filed gap | manual + capture (this is inherently a live dogfood run, not a unit test) | N/A — evidence captured per 13-06 precedent (commands, `--json` capture, PR link, failure classification) | N/A |
| CODE-01 (driver parity, D-04) | Codex argv/env/interactivity unchanged | unit | `cargo test -p devflow-core --lib codex_and_pi_drivers_reproduce_legacy_behavior codex_wraps_prompt_in_exec_and_json codex_grants_writable_roots_for_worktree_git_metadata codex_disables_signing_via_env_others_do_not codex_define_and_plan_require_an_existing_artifact` | ✅ `crates/devflow-core/src/agents/mod.rs` |
| #147 (D-05/D-06/D-07) | `resume --agent` mutates only agent, saves before relaunch, emits handoff event | unit + integration | new tests in `pipeline_launch.rs`; extend `--help` snapshot | ❌ Wave 0 — new tests, new snapshot regen |
| #147 (D-08) | Refuses unsafe handoff before mutation | unit | new negative-control test: `resume --agent codex` at `Stage::Define` in Auto mode with no `-CONTEXT.md` on develop must refuse and leave `state.agent` unchanged | ❌ Wave 0 |
| #148 (D-10/D-14) | `--from-devflow` string is gone; new command uses real flags | unit | rewrite `cron_instruction_hints_include_hermes_command_per_phase`, `cron_hint_line_appends_sanitized_reset_when_retry_after_present`, `cron_hint_line_omits_reset_fragment_when_retry_after_empty` (`commands.rs:4139-4200`) + new negative-control asserting absence | ✅ files exist, ❌ assertions need rewriting |
| #148 (D-12) | Schedule is unambiguous regardless of scheduler timezone | unit | positive: an ISO-with-offset schedule round-trips; negative control: the OLD `M H D M W` UTC-field approach demonstrably fires at the wrong instant when interpreted in a non-UTC zone (keep/adapt `cron_schedule_normalizes_negative_offset`-style test as the "why this was wrong" regression) | ✅ `ship.rs` schedule tests exist as a base, ❌ new ISO-render tests needed |
| #148 (D-13) | Unparseable retry time still fails closed | unit | existing `cron_instructions_reject_unparseable_retry_time` (`ship.rs`) — re-verify after the render-function swap | ✅ `crates/devflow-core/src/ship.rs` |
| #153 (D-15/D-16) | Cron record deleted only after genuine relaunch; survives a failed launch | unit + integration | new test: stub a failing `spawn_monitor`/agent binary, assert record survives; new test: successful relaunch deletes it | ❌ Wave 0 |
| #153 (D-17) | Ship completion deletes any remaining record | integration | new test at `finish_workflow_with_gate_timeout` call site | ❌ Wave 0 |
| #153 (D-18) | Deletion emits an audit event | unit | assert event presence/fields on both delete paths | ❌ Wave 0 |
| Phase success criterion 3 | No regression to Codex driver behavior | full suite | `cargo test --workspace` (assert `N passed`, `0 failed`, per this repo's own false-green-avoidance rule) | ✅ existing baseline |

### Sampling Rate
- **Per task commit:** targeted `cargo test -p devflow --bin devflow <new/changed test names>`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** full suite green + `cargo run -q -p devflow -- --help` diffed against the
  regenerated snapshot, before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `pipeline_launch.rs` — handoff mutation tests (D-06/D-07/D-08 positive + negative)
- [ ] `commands.rs` — rewritten cron-hint tests + `--from-devflow`-absence negative control
- [ ] `ship.rs` — ISO-with-offset schedule render + round-trip tests, negative control for the old
      UTC-cron-field approach
- [ ] `pipeline_launch.rs` / `pipeline_gate.rs` — cron-deletion trigger tests (success deletes,
      failure preserves, ship-completion belt-and-braces)
- [ ] `crates/devflow-cli/tests/snapshots/devflow-help.txt` — regenerate after the `--agent` flag
      lands on `Resume`
- [ ] Codex dogfood evidence capture directory (per 13-06/34-evidence precedent): commands run,
      `--json` capture, PR link, failure classification if any gap surfaces

*(Framework itself needs no install — `cargo test` is already the project's only test runner.)*

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | No new auth surface — `resume --agent` is a local CLI flag, not a network-facing credential path |
| V3 Session Management | no | N/A |
| V4 Access Control | no | DevFlow has no multi-user access model; a handoff is a local operator action on their own phase state |
| V5 Input Validation | yes | `--agent <AGENT>` reuses `AgentKind`'s existing `FromStr`/clap value parser (already validated, `[VERIFIED: crates/devflow-cli/src/main.rs:54-55]` — `Start`'s `--agent` uses the same type) — no new parsing surface |
| V6 Cryptography | no | Unaffected — Codex's signing-disable env vars are pre-existing and unchanged by this phase |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Command injection into the printed/executed `hermes cron create` string via a hostile `project_root` path or `phase` value | Tampering | `ship.rs`'s existing `shell_quote` helper (`ship.rs:374-389`) already quotes the project path for the embedded `command` field — the #148 rewrite must keep routing the project path through it, not string-interpolate raw |
| A handoff silently changing the audit trail's agent attribution | Repudiation | D-07's mandatory `handoff` event in `.devflow/events.jsonl`, following the same "log loudly" contract as `legacy_claude_launch_forced` |
| A crafted `retry_after` string (agent-controlled, since it comes from parsed agent output) producing a malformed or every-minute schedule | Tampering / Denial of Service | Already mitigated by the existing WR-06 fail-closed rule (`cron_schedule_from_retry_after` returns `None`/empty rather than degrading to `"* * * * *"`) — the #148 ISO-render swap must preserve this same fail-closed behavior for unparseable input (D-13) |

## Sources

### Primary (HIGH confidence — read directly this session)
- `crates/devflow-core/src/agents/codex.rs` — Codex driver launch/env/interactivity contract
- `crates/devflow-core/src/agents/mod.rs` — `AgentDriver` trait, conformance suite, existing Codex tests
- `crates/devflow-core/src/ship.rs` — `CronInstructions` schema, schedule math, delete primitives
- `crates/devflow-core/src/state.rs` — `State`/`AgentKind` field list
- `crates/devflow-cli/src/main.rs` — `Command::Resume`/`Command::Start` CLI definitions
- `crates/devflow-cli/src/pipeline_launch.rs` — `resume`, `launch_stage`, `spawn_agent_and_record`
- `crates/devflow-cli/src/pipeline_gate.rs` — `finish_workflow_with_gate_timeout`
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `handle_rate_limited_outcome`
- `crates/devflow-cli/src/preflight.rs` — `preflight_interactivity_check`, `run_preflight`
- `crates/devflow-cli/src/commands.rs` — `cron_hint_line`, its existing unit tests
- `crates/devflow-core/src/recover.rs` — existing `delete_cron_instructions` call sites
- `crates/devflow-cli/tests/help_snapshot.rs` — `--help` snapshot guard
- `~/Github/hermes-agent/cron/jobs.py` (installed Hermes source, v0.20.5) — `parse_schedule`,
  `compute_next_run`, `create_job` repeat semantics
- `~/Github/hermes-agent/hermes_time.py` — `HERMES_TIMEZONE`/config.yaml timezone resolution
- `~/Github/hermes-agent/hermes_cli/cron.py` — `cron_create` argument surface
- Live CLI probes this session: `hermes cron create --help`, `hermes --version`, `codex --version`
- `gh issue view 147/148/153 --json title,body,state` — verbatim issue bodies fetched this session

### Secondary (MEDIUM confidence)
- MemPalace recall (`44-MEMORY-RECALL.md`) — Phase 13/21/28 precedents for dogfood evidence bar,
  cron-hint composition rule, resume/handoff machinery shape

### Tertiary (LOW confidence)
- None — every substantive claim in this document traces to a file read or command run this session.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies, all internals read directly
- Architecture: HIGH — every integration point cited to a specific file:line read this session
- Pitfalls: HIGH — five of five pitfalls each backed by a specific test/file/CLI-help artifact read
  or run this session, not inferred

**Research date:** 2026-08-26
**Valid until:** 30 days (stable internal Rust codebase; the one external-surface dependency —
installed Hermes CLI behavior — should be re-checked with `hermes cron create --help` if the
operator's Hermes install has since been upgraded, since its cron grammar is not versioned/pinned
by DevFlow)
