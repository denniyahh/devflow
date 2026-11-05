# Phase 51: Rate-Limit Agent Failover - Research

**Researched:** 2026-11-05
**Domain:** In-repo Rust: DevFlow's in-supervisor advance path, agent substitution, rate-limit
classification, idle-timeout side channel, gate notify plumbing, source-scan tests
**Confidence:** HIGH for the code paths and semver facts (read and probed this session); MEDIUM for
the proposed mechanisms (they are recommendations within the locked decisions, not tested code)

## Summary

The phase is almost entirely a **re-routing** inside one function. Every rate limit already reaches
the same spot: `advance_with`'s `Action::AutoResume` arm (`pipeline_launch.rs:3736-3741`), which
runs **inside the stage supervisor process** (`run_monitor` → `advance_for_stage`,
`pipeline_launch.rs:1608-1613`) and today calls `handle_rate_limited_outcome`
(`pipeline_outcomes.rs:89-157`). The substitution logic it needs already exists inline in
`resume_loaded` (`pipeline_launch.rs:3174-3220`): full preflight on a candidate state, binary check,
`health`, `commit` of `state.agent`, `agent_handoff` event. Failover = extract that block into one
shared function, call it from the AutoResume arm **before** the infra bump (D-06), then relaunch the
same stage with `launch_stage(state, prompt, Some(stage))`, exactly as `handle_ambiguous_outcome`
already re-drives a stage from the same arm (`pipeline_launch.rs:2901-2935`).

The hard parts are not the hop itself but what the relaunch inherits: (1) **per-agent state** — the
recorded `state.canary` is reused without re-running (`canary_gate`, `pipeline_launch.rs:552-576`),
so a Claude `Confirmed` would admit an Antigravity launch; (2) **the interrupted intent** — `launch_stage`
rebuilds `StageIntent::for_stage_in_project`, which yields `Code { fix: None }` and, for Plan, a
prompt that treats any `PLAN.md` as "already done" (`prompt.rs:387-410`, `prompt.rs:516-535`);
(3) **semver** — the published `devflow-core` 3.0.0 must stay minor-compatible, and two of the
obvious edits are major breaks (verified with `cargo semver-checks` below).

**SUPV-01 (criterion 3), traced:** the failover arm never consults liveness about the *current*
run (it **is** the current run, mid-advance; no `reserve_launch`). Its only Phase 50 liveness read
is the one every in-supervisor launch makes: `wait_for_publish` probing the **successor's** socket
(`monitor.rs:2440-2462`). The cron arm's later `devflow resume` goes through `reserve_launch` →
`refuse_launch_over_a_live_run` (`commands.rs:927-1000`, `4601-4628`), which does probe the recorded
supervisor. So failover depends on SUPV-01 exactly as much as any stage-to-stage launch does — no
more. The withholding experiment in D-12 must include an ordinary relaunch control, because
withholding the publish probe breaks every launch.

**Primary recommendation:** Extract `hand_off_agent(...)` from `resume_loaded`; branch to it at the
top of the AutoResume `RateLimited` arm when a fallback is configured and unused; invalidate
`state.canary` inside it; persist the interrupted Code fix type in `State`; keep every new
`devflow-core` public change additive (fields on `#[non_exhaustive] State` only, never on
`IdleTimeoutRecord`, `StageIntent` or `Action`).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Revised 2026-11-05 after an adversarial review of the first draft** (codex gpt-6.1-sol and
DeepSeek Flash via pi; agy dropped on a quota error). Every finding below marked *(review)* was
checked against source before it changed a decision. The review files are not committed; the
findings that changed something are summarised in `51-DISCUSSION-LOG.md`.

#### Fallback configuration surface
- **D-01:** One fallback agent, given as `--fallback-agent <AGENT>` on `devflow start`, and on
  `devflow resume` of a run that is not live (stopped, or paused on the cron arm), persisted in
  `state-NN.json` (the same persisted-flag pattern as `legacy_claude_launch`). No `devflow.toml`
  key and no environment variable: issue #180 and 999.41 argue against another config × env pair.
  Not an ordered list. *(review)* A fallback **cannot** be armed on a live run: `resume` refuses
  over one (`refuse_launch_over_a_live_run`, `crates/devflow-cli/src/commands.rs`), and this phase
  does not add a non-launching write path. Setting a fallback with `resume` clears the
  consumed mark (D-05), so a run can be re-armed after a hop.
  — **Reversibility:** costly — the persisted field and the CLI flag become part of the state
  format and operator surface; widening to a list later is additive, removing it is not.
- **D-02:** A fallback equal to the primary agent is refused at `start`/`resume` with a message
  naming both.
- **D-03:** *(review: revised)* Launch-time validation is **best effort, not a guarantee**.
  `generic_preflight_checks` is stage-dependent (`major_bump_check_applies` is Ship-only;
  `unattended_launch_check_applies` covers only Define and Code, `crates/devflow-cli/src/preflight.rs`),
  so it cannot be fully evaluated at `start` for a handoff that may happen at any stage. At launch,
  check what is stage-independent or knowable: the binary (`ensure_agent_binary`), the driver's
  `health`, and the fallback against the Code-stage launch-shape condition. Consequence: under
  `--mode auto`, `unattended_launch_shape_condition` holds only for `claude` and `antigravity`, so
  any other fallback is refused at launch in auto mode; that guard is **not** widened here. The
  full stage-dependent bundle runs again at handoff (D-07).

#### Handoff mechanism
- **D-04:** On `RateLimited`, if a fallback is configured and unused, perform the substitution
  `resume_loaded` does today (`crates/devflow-cli/src/pipeline_launch.rs`): full preflight bundle
  for the current stage, binary check, health, `commit` the agent change, emit `agent_handoff`
  with `"reason": "rate_limited"`, then relaunch the same stage at once. One shared function serves
  both `resume --agent` and failover. Two additions *(review)*:
  - **Canary evidence is per agent.** `canary_gate` reuses a recorded `state.canary` without
    re-running it, and the canary launcher differs per agent (`canary_launcher_for`, same file).
    The substitution must invalidate the recorded canary so the fallback's own canary runs. This
    also fixes `resume --agent` today. Negative control: a Claude `Confirmed` canary must not
    admit an Antigravity launch whose canary returns `Absent`.
  - **A relaunch that fails after the commit is never silent.** `resume_loaded` re-marks the
    phase stopped when its launch fails (44-CORE-REVIEW finding 2a); failover needs an equivalent
    recovery, ending in a gate or the cron arm, never in a phase recorded as handed off with
    nothing running. Tested by injecting a spawn failure after the commit.
- **D-05:** One hop per run. The handoff marks the fallback consumed in state. A `RateLimited`
  from the fallback takes the cron arm. The run stays on the fallback; no automatic switch back.
  A later cron resume relaunches with the saved (fallback) agent.
- **D-06:** *(review: revised)* The failover branch is taken **before** the infra-ceiling check and
  does **not** bump `infra_failures`. D-05's one-hop mark is what bounds failover. Reason: the
  ceiling (`MAX_INFRA_FAILURES` = 5) is shared with `Ambiguous` re-drives, so charging failover to
  it would let earlier transport cancels cost the run its configured fallback. The cron arm keeps
  today's bump-then-check behaviour. Boundary test: with `infra_failures` at
  `MAX_INFRA_FAILURES - 1` and an unused fallback, a rate limit hands off instead of gating.
- **D-07:** If the handoff is refused at runtime (the current stage's preflight, binary or
  health fails), take the cron arm and emit an event naming the refusal reason. Never a silent
  skip. Accepted limit: because D-03 cannot check every stage at launch, a refusal here can still
  end the night; the event makes it visible.
- **D-08:** Zero-commit case (issue #180's open question): continue with the fallback.

#### Mid-stage continuation
- **D-09:** *(review: revised)* The fallback must continue **the interrupted operation**, not the
  stage's default prompt. Two concrete gaps make the default unsafe:
  - **Plan:** `idempotent_stage_prompt` (`crates/devflow-core/src/prompt.rs`) treats the mere
    presence of a `PLAN.md` as a completed stage and forbids touching it. A Plan rate-limited
    midway can leave a partial plan file, which the fallback would then accept as done. A Plan
    failover must not take that shortcut.
  - **Code:** a relaunch rebuilds `StageIntent::Code { fix: None }`, but a Code pass may have been
    an audit-fix loop-back (`FixType::AuditFix`, `crates/devflow-cli/src/pipeline_outcomes.rs`),
    which runs a different workflow. The interrupted intent, including its fix type, must be
    persisted and replayed to the fallback.
  Acceptance (falsifiable): a Plan failover with an incomplete plan file on disk does not report
  the stage done without completing it (negative control: a complete plan still short-circuits);
  a Code failover during an audit-fix pass runs the audit-fix workflow (negative control: a plain
  Code failover runs execute-phase). The mechanism is left to the planner.
- **D-10:** The worktree is handed over as-is: no reset, stash or clean of uncommitted changes.
  This matches today's cron arm, which already restarts on whatever tree the rate-limited agent
  left. It differs from the idle-timeout rule (`outcome_policy.rs`: an idle timeout is terminal
  because the run's extent is unknown) because a rate limit is a reported stop. The tree state at
  handoff (clean or dirty, commit count) is recorded in the `agent_handoff` event.

#### Proving both arms and the SUPV-01 dependency
- **D-11:** Both arms are asserted by hermetic end-to-end tests that drive the real monitor with
  stub agents emitting a real rate-limit envelope: fallback configured → `agent_handoff` plus a
  same-stage relaunch under the fallback that reaches normal completion; no fallback → cron
  instructions written and no handoff. Each test asserts the opposite arm's absence as its
  negative control.
- **D-12:** *(review: revised)* "Liveness unavailable" is not a switch that exists. The review
  traced the failover relaunch through `launch_stage` inside the running supervisor, not through
  `reserve_launch`, which is where foreground launches consult liveness; the cron arm's scheduled
  `devflow resume` does go through `reserve_launch`. The planner must first trace which liveness
  reads each arm actually makes and record that trace. Then, for each read found, withhold that
  specific answer, with a healthy baseline and an ordinary relaunch under the same intervention as
  controls, so a failure can be attributed to failover rather than to launching being disabled.
  If the failover arm makes no liveness read, that trace **is** the measured result for
  criterion 3 and is recorded as such. Do not record "works" from a test whose outcome could not
  have differed.
- **D-13:** A real provider rate limit cannot be produced on demand, so criterion 1's proof is the
  hermetic e2e in D-11. A natural rate limit during this phase's own dogfood run is recorded as
  additional evidence, not a gate.

#### Keeping the removed handoff dead (criterion 4)
- **D-14:** A Rust source-scan test (the `doc_check.rs` pattern, comments stripped) asserts no
  `rebase_in` definition or call, no `sequentagent` command and no `n` verb. *(review)* Verbs are
  clap variants of `enum Command` (`crates/devflow-cli/src/main.rs`): the scan checks variant
  names, `#[command(name = …)]` and `alias` attributes, not free text. The test-code literal
  `"sequentagent"` in `ship.rs`'s negative assertion must not trip it. Negative control: a
  fixture source containing a `Sequentagent` variant or an `alias = "n"` fails the scan.

#### Folded backlog items
- **D-15 (999.102):** *(review: revised)* `IdleTimeoutRecord` (`agent_result.rs`) has **no**
  free-text field (999.102's premise is stale), and the idle-timeout reason is built from that
  side-channel record, not from the capture. So the monitor captures `terminal_reason`,
  `errors[]` and `origin.kind` from the last `result` event when it writes the record, as new
  **optional** fields (`#[serde(default)]`), and the reason string and gate context include them
  when present. Tests: fields present → named in the reason; fields absent (older CLI, killed
  before a `result`) → today's message; **a pre-51 record on disk still parses** (a required
  field would turn every existing record into the "unreadable" message).
- **D-16 (999.71):** Build the measurement harness the entry describes and archive the
  measurements file. The entry is partly stale: `claude_stream_events` no longer exists, and
  `ParsedCapture` (`agent_result.rs`) now reports torn lines via `line_shapes`. *(review)* The
  rate-limit classifier is called with `ParsedCapture::parse(&raw).events` only, so it cannot see
  a torn final line today. In scope: the classifier is made to receive the torn-line signal and
  is tested with a torn final line (a torn rate-limit event must not be read as a clean,
  non-rate-limited ending). The production-rate measurement is a separate deliverable and is not
  a gate on the phase's other criteria.
- **D-17 (999.154):** *(review: revised)* Part of this entry is superseded by Phase 50.1:
  `advance` already commits the notify command (`commit_notify_env`), and gate entry commits only
  `gate_pending` through `commit`, which reloads the on-disk state. Reproduce WR-12 and IN-10
  against current code before fixing them; close either one that no longer reproduces, with the
  reproduction attempt as evidence. WR-13 stands as is: add a `resume` test with the variable
  unset that asserts the saved command is kept, which is the arm a scheduled rate-limit resume
  takes. Correct the `strip_notify_env` (`monitor.rs`) and `gate_notify_cmd` (`state.rs`) doc
  comments if they still overstate the coverage.

### Claude's Discretion
- Exact field names in `state-NN.json` and the event payload shape, within D-01/D-04/D-10.
- Where the shared handoff function and the persisted interrupted intent live.
- Plan split and wave layout.

### Deferred Ideas (OUT OF SCOPE)
- Driver-owned rate-limit detection for Pi, OpenCode, Hermes and Antigravity (issue #180's
  "rate-limit detection is per-agent" note; belongs in the driver contract).
- An ordered list of fallback agents (more than one hop).
- Widening `unattended_launch_shape_condition` so non-Claude/Antigravity fallbacks can run in
  `--mode auto`.
- Automatic switch back to the primary agent after its limit resets.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SUPV-02 | An agent that exhausts its token budget mid-stage continues under a configured fallback agent, in the same worktree on the same branch, with no manual conflict resolution. Both exhaustion answers coexist; re-test the SUPV-01 dependency. | AutoResume arm location and in-supervisor call chain (§Architecture); `resume_loaded` substitution to extract (§Pattern 1); per-agent state to reset (§Pitfall 1); interrupted-intent gap (§Pitfall 2/3); liveness trace for criterion 3 (§Pattern 4); source-scan pattern for criterion 4 (§Pattern 5); semver constraints for a 3.1.0 minor (§Pitfall 4); named tests per criterion (§Validation Architecture) |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- Crates: `devflow-core` (lib, **published on crates.io** — a changed/removed `pub` item or variant
  is a semver change; prefer additive; run `cargo semver-checks check-release -p devflow-core`
  before committing one) and `devflow-cli` (package name **`devflow`**, binary only).
- Test commands: `cargo test -p devflow-core --lib`; `cargo test -p devflow --bin devflow` (never
  `-p devflow --lib`); full suite `scripts/check.sh all`; container parity
  `scripts/check-in-container.sh`.
- Gates: `cargo test <filter>` exits 0 on zero matches. Gate on **one named test per command**:
  `cargo nextest run -p <pkg> --lib --no-tests=fail -E 'test(=module::path::name)'` (`--bin devflow`
  for the CLI), chained with `&&`; never `-E 'test(=a) | test(=b)'`.
- `rg -c`/`--count` print nothing on zero matches: check "no X left" with a Rust source-scan test
  (like `doc_check.rs`), not a shell count. Prefer a named Rust test to any shell pipeline.
- Every literal `std::env::var("DEVFLOW_…"` / `var_os(` / `env_value(` read under `crates/` (tests
  included) and every `Command` variant must be named in README/ARCHITECTURE/CONTRIBUTING/OPERATIONS.md
  or `docs/guides/`, or allowlisted in `doc-check-allowlist.toml` **in the same change**.
- `crates/devflow-cli/build.rs` embeds commit + dirty flag: never compare a rebuilt binary's sha256.
- `core.fileMode=false`: set exec bits with `git add <f> && git update-index --chmod=+x <f>`.
- Phases run in `.worktrees/phase-N`; never run concurrent git ops on the main checkout while an
  executor holds the tree.
- Dogfooding: Phase 51 runs via `devflow start --phase 51 --agent claude --mode supervise --until plan`;
  external adversarial review (codex + agy) after Plan, before Code; Ship blocked (`yes_ship`
  false); close-out by hand with `scripts/cut-pr-branch.sh`; any DevFlow defect the run hits is a
  `999.x` backlog entry.
- Modifying CI/hooks/devcontainer/tooling deps → update `.planning/user/DEV-SETUP-CHECKLIST.md` in
  the same commit.
- Global (operator): `cargo clippy --workspace --all-targets -- -D warnings` before completion;
  tests must assert non-trivial properties with negative controls; no `std::env::set_var` in tests
  (configure spawned children); no regex-alternation assertions; behavior-led test names (no review
  round IDs in test names).

## Architectural Responsibility Map

DevFlow is a CLI + detached supervisor, not a web stack; tiers are mapped to its own layers.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Rate-limit classification | devflow-core `agent_result` (Layer 1 cascade) | devflow-core `monitor` (idle-timeout suppression) | Classification already lives here; failover only consumes `AgentStatus::RateLimited` |
| Failover decision (fallback configured & unused?) | devflow-cli `pipeline_launch` AutoResume arm (in-supervisor advance) | — | Must not live in core `outcome_policy`: `Action` is an exhaustive pub enum (adding a variant is a major break) |
| Agent substitution (preflight, binary, health, commit, event) | devflow-cli shared handoff fn (extracted from `resume_loaded`) | devflow-cli `preflight` | One function serves `resume --agent` and failover (D-04) |
| Same-stage relaunch | devflow-cli `launch_stage` → `spawn_agent_and_record` → core `monitor::spawn_monitor` | core `supervisor::probe` (publish wait) | Same path as Ambiguous re-drive and stage transitions |
| Persisted run config (fallback, consumed mark, interrupted fix) | devflow-core `State` (`#[non_exhaustive]`, `#[serde(default)]` fields) | — | Each stage launch runs in a separate process; only state survives |
| Continuation prompt | devflow-core `prompt` (additive pub fn) rendered per driver | devflow-cli (selects it) | Prompt text is per driver (`render_claude_style` vs `render_workflow_style`) |
| Cron arm (no fallback) | devflow-cli `handle_rate_limited_outcome` | devflow-core `ship::{build_single_agent_cron_instructions, write_cron_instructions}` | Unchanged except it becomes the "else" arm |
| Removed-handoff scan | devflow-core `doc_check` (`#[cfg(test)]`, scans all of `crates/`) | — | Existing comment-stripped scan pattern with planted controls |
| Idle-timeout cause fields | devflow-core `monitor` (write) + `agent_result` (read) | — | Side-channel record is the verdict source |
| Gate notify (999.154) | devflow-cli `pipeline_gate`/`pipeline_launch` + `tests/gate_notify_persist_e2e.rs` | devflow-core `gates` | Existing e2e harness for resume arms |

## Standard Stack

No new external dependencies. Everything is built from crates already in the workspace.

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde / serde_json | workspace | State and side-channel (de)serialization, `#[serde(default)]` additive fields | Already the persistence layer for `state-NN.json` and the idle-timeout record |
| clap (derive) | workspace | `--fallback-agent` on `Start`/`Resume` | `Command` enum is clap-derive (`main.rs:47`) |
| tempfile | workspace (dev) | Hermetic fixtures | Used by every existing pipeline test |

### Tooling (verified installed this session)
| Tool | Version | Purpose |
|------|---------|---------|
| cargo | 1.97.1 | build/test |
| cargo-nextest | 0.9.150 | named-test gates (`--no-tests=fail -E 'test(=…)'`) |
| cargo-semver-checks | 0.51.0 | `check-release -p devflow-core` before any pub change |
| cargo-mutants | 27.1.0 | optional: mutate the AutoResume branch / `apply_notify_env` call |
| cargo-llvm-cov | 0.9.1 | optional coverage of the new branch |
| cargo-deny | 0.20.2 | unchanged deps; not needed unless deps change |

**Installation:** none.

## Package Legitimacy Audit

No external packages are installed or added by this phase. **Packages removed due to [SLOP]:** none.
**Packages flagged [SUS]:** none.

## Architecture Patterns

### System Architecture Diagram

```
 agent (primary, e.g. claude) ──stdout──▶ capture .devflow/phase-NN-stdout
          │ exits (or idle-timeout kills it; monitor suppresses the idle record
          │ when capture_shows_rate_limit_denial)                     
          ▼
 stage supervisor (__monitor, run_monitor) ── in-process ──▶ advance_for_stage
          │                                                       │
          │                                        evaluate_layer1 → AgentResult
          │                                                       │
          │                                        decide_action: RateLimited ⇒ AutoResume
          │                                                       ▼
          │                                  ┌── AutoResume arm (pipeline_launch.rs:3736) ──┐
          │                                  │ Ambiguous → handle_ambiguous_outcome (re-drive)│
          │                                  │ RateLimited:                                  │
          │                                  │   fallback set & unused? ──no──▶ CRON ARM     │
          │                                  │        │yes                    (bump infra,  │
          │                                  │        ▼                        write cron,   │
          │                                  │   hand_off_agent(candidate)     event, Ok)    │
          │                                  │   preflight/binary/health ─refused─▶ event +  │
          │                                  │        │ok                        CRON ARM   │
          │                                  │   commit {agent, consumed, canary=None}       │
          │                                  │   emit agent_handoff{reason:rate_limited,     │
          │                                  │        tree_dirty, commit_count}              │
          │                                  │        ▼                                       │
          │                                  │   launch_stage(continuation prompt, Some(stage))│
          │                                  │        │Err ─────────────────▶ event + CRON ARM │
          │                                  └────────┼──────────────────────────────────────┘
          │                                           ▼  or gate
          │                         spawn_next_supervisor → spawn_monitor
          │                         → wait_for_publish: supervisor::probe(successor socket)  ◀── the one SUPV-01 read
          │                         → publish_record (ownership fence) → record_successor
          ▼
 run_monitor marks itself HandedOff, exits      fallback agent runs SAME stage, SAME worktree/branch
                                                           │ exits → its supervisor advances normally
 CRON ARM later: cron runs `devflow resume --phase N` → claim_foreground_holder → reserve_launch
   → refuse_launch_over_a_live_run (probes recorded/pending sockets) ◀── SUPV-01 read on the cron arm
```

### Recommended Change Map
```
crates/devflow-core/src/
├── state.rs           # + fallback_agent: Option<AgentKind>, fallback_consumed: bool,
│                      #   + interrupted Code fix (Option<FixType>), all #[serde(default)]
├── prompt.rs          # + serde derives on FixType (additive); + pub continuation-prompt fn(s)
├── agent_result.rs    # torn-line signal into the stream rate-limit check (private change);
│                      #   idle-timeout cause read via a crate-private wrapper struct
├── monitor.rs         # write cause fields at idle timeout; doc-comment fix (strip_notify_env)
└── doc_check.rs       # + removed-handoff source scan with planted controls
crates/devflow-cli/src/
├── main.rs            # --fallback-agent on Start and Resume
├── commands.rs        # start(): D-02/D-03 validation, persist; dry-run prints it
├── pipeline_launch.rs # extract hand_off_agent from resume_loaded; AutoResume failover branch;
│                      #   resume --fallback-agent (clears consumed mark)
├── pipeline_gate.rs   # persist interrupted fix in prepare_loop_back_to_code
└── pipeline_outcomes.rs # handle_rate_limited_outcome unchanged (cron arm)
crates/devflow-cli/tests/gate_notify_persist_e2e.rs  # + resume arm with the variable unset
OPERATIONS.md, docs/guides/unattended-mode.md, README.md:110   # flag, events, cron row
```

### Pattern 1: Extract the substitution, keep resume's behaviour byte-identical

**What:** `resume_loaded` lines 3174-3220 become `hand_off_agent(project_root, &mut state, to,
reason) -> Result<(), HandoffRefusal>` (CLI-private). It builds `candidate = state.clone()` with
`agent = to` and `canary = None`, runs `generic_preflight_checks(project_root, &candidate)`,
`ensure_agent_binary(agent_program(to))`, `driver_for(to).health(&candidate)`, then **one**
`commit` that sets `agent`, clears `canary`, and (failover only) sets the consumed mark, then emits
`agent_handoff`. `resume_loaded` calls it with reason `"resume --agent"` and keeps mapping a refusal
to its existing `CliError::Message("handoff to {requested} refused …")` text.

**Verified current shape** `[VERIFIED: crates/devflow-cli/src/pipeline_launch.rs:3174-3220]`:
`generic_preflight_checks(project_root, &candidate).map_err(|reason| CliError::Message(format!("handoff to {requested} refused at saved {} stage: {reason}", state.stage)))?;` …
`commit(&mut state, |s| s.agent = requested)?;` …
`events::emit(project_root, phase, "agent_handoff", serde_json::json!({ "stage": …, "from_agent": …, "to_agent": …, "reason": "resume --agent", }));`

**When to use:** both callers. Failover passes `"rate_limited"` and adds D-10's tree fields.

### Pattern 2: Failover branch at the top of the RateLimited arm (before the infra bump)

**Verified current arm** `[VERIFIED: crates/devflow-cli/src/pipeline_launch.rs:3736-3741]`:
```rust
Action::AutoResume => match result.status {
    agent_result::AgentStatus::Ambiguous => {
        handle_ambiguous_outcome(project_root, &mut state, stage, result.reason)
    }
    _ => handle_rate_limited_outcome(project_root, &mut state, phase, stage, result.reason),
},
```
`handle_rate_limited_outcome` bumps first, then checks the ceiling
`[VERIFIED: crates/devflow-cli/src/pipeline_outcomes.rs:96-102]`:
`commit(state, |s| { s.infra_failures = s.infra_failures.saturating_add(1) })?;`
`if state.infra_failures >= mode::MAX_INFRA_FAILURES { return gate_or_abort_infra(…); }`
with `pub const MAX_INFRA_FAILURES: u32 = 5;` `[VERIFIED: crates/devflow-core/src/mode.rs:68]`.

**Recommended shape** (sketch; names are the planner's within D-01/D-04):
```rust
// Source: shape of handle_ambiguous_outcome (pipeline_launch.rs:2901-2935), which already
// re-drives the same stage from this arm via launch_stage(state, None, Some(stage)).
_ => match unused_fallback(&state) {           // Some(agent) iff set && !consumed
    Some(to) => fail_over(project_root, &mut state, phase, stage, to, result.reason),
    None => handle_rate_limited_outcome(project_root, &mut state, phase, stage, result.reason),
},
// fail_over:
//   hand_off_agent(.., to, "rate_limited") — Err(refusal) => emit "agent_handoff_refused"
//       {stage, to_agent, reason} then handle_rate_limited_outcome(..) (D-07)
//   launch_stage(state, continuation_prompt(state, stage), Some(stage))
//       — Err(e) if !is_not_owner(&e) => emit "agent_handoff_relaunch_failed" then
//         handle_rate_limited_outcome(..) or gate_or_abort_infra(..) (D-04: never silent)
//       — Err(e) if is_not_owner(&e) => return Err(e) (superseded supervisor: not ours to mark)
```
Do not call `commit`/`close_gate` inside a `fenced` closure; `fenced` is non-reentrant on the
state write lock (`pipeline_launch.rs:1102-1107`).

### Pattern 3: Persist the interrupted intent, re-render with the fallback's driver

`launch_stage` renders `driver.render_prompt(&StageIntent::for_stage_in_project(state.stage, …))`
from the **current** `state.agent` when `prompt_override` is `None`
`[VERIFIED: crates/devflow-cli/src/pipeline_launch.rs:2861-2868]`, and
`for_stage_in_project` maps `Stage::Code => StageIntent::Code { phase, fix: None }`
`[VERIFIED: crates/devflow-core/src/prompt.rs:206-212]`. The fix type is not persisted today:
`prepare_loop_back_to_code` only returns a rendered string
`[VERIFIED: crates/devflow-cli/src/pipeline_gate.rs:173-219]`.

- **Persist the intent, never the rendered string.** Renderers differ per driver: Claude, Hermes,
  OpenCode, Antigravity use `render_claude_style`; Codex and Pi use `render_workflow_style`
  `[VERIFIED: crates/devflow-core/src/agents/{claude,hermes,opencode,antigravity,codex,pi}.rs render_prompt]`.
  A Claude-rendered `/gsd-audit-fix` string handed to Codex is the wrong instruction.
- `FixType` today: `#[derive(Debug, Clone, Copy, PartialEq, Eq)] #[non_exhaustive] pub enum FixType { AuditFix, GapsOnly, FullExecute }`
  `[VERIFIED: crates/devflow-core/src/prompt.rs:244-258]` — no serde derives. Adding
  `serde::Serialize, serde::Deserialize` is additive (probe below), so a `#[serde(default)]
  pub <field>: Option<FixType>` on `State` is semver-safe. Set it in `prepare_loop_back_to_code`;
  clear it on every ordinary Code entry (transition into Code / Validate pass) so a stale fix never
  replays into a later plain Code pass.
- **Plan:** both renderers short-circuit on any `{padded}-*PLAN.md`
  `[VERIFIED: crates/devflow-core/src/prompt.rs:387-410 (idempotent_stage_prompt), 516-535 (workflow_plan_prompt)]`.
  Recommended mechanism `[ASSUMED — recommendation, falsifiable by the D-09 tests]`: at handoff,
  decide "plan complete" from git, not from presence — the fallback gets the default (short-circuiting)
  prompt only if every `{padded}-*PLAN.md` in the worktree is tracked and unmodified
  (`git status --porcelain -- .planning/phases/{padded}-*/`) **and** at least one exists; otherwise
  it gets a new additive `pub fn plan_continuation_prompt(phase, …)` (one per renderer style) that
  says a previous agent was interrupted, existing PLAN files may be partial, and the plan phase must
  be completed and committed before reporting success. Do **not** add a `StageIntent` variant or
  field (major break; see Pitfall 4).

### Pattern 4: SUPV-01 trace (criterion 3) — what each arm actually reads

| Arm | Liveness reads (Phase 50/50.1 machinery) | Source |
|-----|------------------------------------------|--------|
| Failover (in-supervisor) | `stopped_at_safe_point(state, 2)` (stop token, not a socket); `take_held_reservation`/`monitor::Reservation::new` (creates the successor's liveness lock); `wait_for_publish` → `supervisor::probe(successor socket)` until `Liveness::Alive(reply)` with this `launch_id`; `publish_record` ownership fence | `pipeline_launch.rs:2540-2578`; `monitor.rs:2440-2462` |
| Failover (current run) | **None.** No `reserve_launch`, no `refuse_launch_over_a_live_run`, no probe of the current supervisor | `pipeline_launch.rs:2856-2889` (no reserve call on this path) |
| Cron arm (later `devflow resume`) | `claim_foreground_holder`; `reserve_launch` → `refuse_launch_over_a_live_run` → `possibly_live_sockets` (probes recorded and pending sockets) + agent pid check | `pipeline_launch.rs:2983-3002`; `commands.rs:927-1000`, `4601-4628` |

`wait_for_publish` treats Gone, Stale, a probe error or another launch id as "not yet published"
and fails with `MonitorError::NotReady` after the wait `[VERIFIED: crates/devflow-core/src/monitor.rs:2429-2462]`.
**Experiment for D-12:** add a `#[cfg(test)]` seam that makes the publish probe report not-Alive
for one launch (new `DEVFLOW_TEST_*` variable → `doc-check-allowlist.toml` entry with reason, same
form as `DEVFLOW_TEST_HOLD`). Run three cases under the full-monitor harness: (a) baseline,
no seam: failover completes; (b) seam on the failover relaunch: failover relaunch fails and ends in
the cron arm/gate (never silent); (c) control, seam on an **ordinary** stage transition
(Define→Plan) in the same rig: it fails the same way. (b)≡(c) is the measured answer: "failover
needs SUPV-01's publish probe exactly as every launch does; it makes no liveness read about the
current run." Record the trace table above in the phase's verification evidence.

### Pattern 5: Removed-handoff source scan (criterion 4, D-14)

Prior art: `doc_check::no_monitor_pid_field_json_key_or_help_mention_remains`
`[VERIFIED: crates/devflow-core/src/doc_check.rs:500-600]` — tokens assembled with `concat!` so the
scanning file never contains the token contiguously (`const REMOVED_STATE_FIELD: &str = concat!("monitor", "_pid");`
at line 506), an `uncommented_lines` filter (drops lines whose trimmed start is `//`, line 510-515),
pure finding functions over `(PathBuf, String)` inputs, and planted controls in the same test.
`rust_file_paths()` walks every `.rs` under `<workspace>/crates` (lines 86-105); `doc_check` is
`#[cfg(test)] mod doc_check;` in `devflow-core/src/lib.rs:59-60`.

Recommended findings (each a pure fn, each with a planted positive and a planted decoy):
1. `rebase_in` as an identifier in code: `fn rebase_in` or `.rebase_in(` / `rebase_in(` on an
   uncommented line (identifier-boundary check like `declares_pub_field`). Free-text "rebase" must
   **not** match: operator messages legitimately say `git rebase` (`preflight.rs:582`,
   `commands.rs:587`).
2. A `Command` variant named `Sequentagent` (use `doc_check::enum_variants(source, "Command")`,
   which lowercases names, lines 188-221) — scoped to `crates/devflow-cli/src/main.rs`.
3. `#[command(name = "n")]`, `alias = "n"`, `visible_alias = "n"`, `aliases = [… "n" …]` on
   uncommented lines of `main.rs`.
4. Optional: a `sequentagent(` call/definition. Do **not** match the string literal in
   `ship.rs:940` (`assert!(!record.hermes_cron.command.contains("sequentagent"));`) — scope rules
   2-4 to declarations/attributes, not quoted text.

**Comment stripping must handle more than whole-line `//`.** The existing helpers only drop whole
lines; a trailing `// rebase_in` or a `/* … */` block would count. Either strip trailing `//` and
block comments too (respecting string literals), or record the limit explicitly in the test's doc
comment. Planted decoys: a doc-comment line `/// rebase_in` and a trailing-comment line must **not**
trip it; a `Sequentagent {` variant and `alias = "n"` **must**.

**History note:** no `n` verb ever existed under that spelling: `git log -G'alias *= *"n"|name *= *"n"' -- crates`
returns nothing, and the removed verb was `Command::Sequentagent` (`dbab01a`); `rebase_in` was
`GitFlow::rebase_in` (`0833a6c`). `write_rate_limit_cron` was also deleted in `dbab01a` (its
commit message lists it), which is why no function by that name exists today.

### Anti-Patterns to Avoid
- **Deciding failover in `outcome_policy`:** `pub enum Action` has no `#[non_exhaustive]`
  `[VERIFIED: crates/devflow-core/src/outcome_policy.rs:14-27]`; a new variant is a major break.
- **Charging failover to `infra_failures`:** contradicts D-06.
- **Persisting a rendered prompt string:** wrong for a fallback with a different renderer.
- **Writing cron instructions naming `--agent <fallback>`:** rejected in discussion; defers the hop.
- **Reading the fallback from env/config:** D-01 forbids; and an env read would also need doc_check
  coverage.
- **Resetting/stashing the worktree at handoff:** D-10 forbids.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Agent substitution | a second copy of resume's preflight/binary/health/commit | the extracted shared handoff fn | 44-CORE-REVIEW finding 2b: a partial check let a handoff commit then fail at the next gate |
| Same-stage relaunch | a new spawn path | `launch_stage(state, prompt, Some(stage))` | carries archive rollover, canary gate, reservation, publish, cron consumption |
| Cron arm | new cron code | `handle_rate_limited_outcome` as-is | already never-silent on unparseable retry hints |
| Commit count for D-10 | a new `rev-list` | `agent_result::phase_commit_count(project_root, &git_flow, phase) -> Option<u32>` (`agent_result.rs:2563-2567`) | the single implementation; `None` ≠ `0` |
| Removed-code scan | shell `rg -c` | `doc_check` Rust scan | `rg -c` prints nothing on zero matches (CLAUDE.md) |
| Torn-line detection | a new line classifier | `ParsedCapture::torn_json_after_last_matching` (`agent_result.rs:1303-1320`) | already encodes Noise vs TornJson |
| Hermetic monitor e2e | a new harness | 50-13 handoff harness (`handoff_claude_dir`, `run_handoff_child`, `HandoffRun::launch_with`, `pipeline_launch.rs:9120-9240`) with `TEST_MONITOR_FULL_ENV` | already runs the real `run_monitor` + advance tail with reaping |
| Post-commit spawn-failure injection | a new fault hook | the test build of `launch_context()` errors when `DEVFLOW_CACHE_DIR` is absent (`pipeline_launch.rs:1994-2003`); or the agent-free `PATH` trick of `resume_re_marks_stopped_when_launch_stage_fails_outright` | deterministic, already used |

**Key insight:** every mechanism failover needs exists; the defects to avoid are in what is
*inherited* across the hop (canary, intent, counters), not in the hop.

## Runtime State Inventory

Not a rename/refactor phase in the usual sense, but it adds persisted state, so the relevant rows:

| Category | Items Found | Action Required |
|----------|-------------|-----------------|
| Stored data | `.devflow/state-NN.json` gains new `#[serde(default)]` fields; `.devflow/phase-NN-idle-timeout*` record gains optional keys | Code edit only; pre-51 files must still load (tests). No migration (operator rule: no version-to-version migration work) |
| Live service config | None — verified: no fallback config lives outside the repo/state | none |
| OS-registered state | Hermes cron jobs scheduled by the cron arm run `cd {project} && devflow resume --phase {phase}` (`ship.rs:299`); they keep working unchanged and resume with the saved (possibly fallback) agent | none |
| Secrets/env vars | `DEVFLOW_GATE_NOTIFY_CMD` (unchanged name); any new `DEVFLOW_TEST_*` seam needs an allowlist entry | allowlist entry in same change |
| Build artifacts | Installed `devflow` binary must be reinstalled before the dogfood run uses failover (CLAUDE.md) | operator step, not code |

**Live run observed:** the main checkout currently holds a live Phase 51 run
(`.devflow/state-51.json`, a `supervisor-*.live` file). This research did not touch it.

## Common Pitfalls

### Pitfall 1: Per-agent state survives the hop
**What goes wrong:** the fallback launches on the primary's evidence.
**Why:** `canary_gate` reuses `state.canary` when `Some` and only runs the canary when `None`
`[VERIFIED: crates/devflow-cli/src/pipeline_launch.rs:552-576]`; `canary_launcher_for` picks
`AntigravityCanaryLauncher` for Antigravity and `ClaudeCanaryLauncher` for every other agent
`[VERIFIED: pipeline_launch.rs:542-550]`; the gate fires only for stream launches, and
`stream_launch_enabled` is `matches!(agent, AgentKind::Claude | AgentKind::Antigravity) && STREAM_JSON_STAGES.contains(&stage) && !(agent == AgentKind::Claude && legacy_opt_out)`
`[VERIFIED: pipeline_launch.rs:825-829]`. So the live exposure is Claude ↔ Antigravity.
**How to avoid:** clear `canary` in the handoff commit. Also review `session_id` (Claude checkpoint
resume) — harmless today because checkpoint resume requires `state.agent == AgentKind::Claude`
(`pipeline_launch.rs:3566`) and D-02 forbids Claude→Claude, but clearing it at handoff is cheap.
**Warning signs:** `canary_gate` unit test with a pre-recorded `Confirmed` passes for a new agent.

### Pitfall 2: Plan short-circuit accepts a partial plan
**What goes wrong:** the fallback sees `51-01-PLAN.md`, prints success, and the run transitions to
Code — `Stage::Plan => transition(project_root, &mut state, Stage::Code)` with no artifact check
`[VERIFIED: pipeline_launch.rs:3510-3511]`.
**How to avoid:** Pattern 3's git-based completeness test; this gap **also exists today on the cron
arm** (a cron `resume` after a mid-Plan rate limit uses the same prompt). Decide whether the new
continuation prompt also applies to the cron arm's resume; recommend yes only if cheap, otherwise
log a `999.x` entry (CLAUDE.md: a defect found is a backlog entry).

### Pitfall 3: Audit-fix intent lost on relaunch
**What goes wrong:** a Code pass that was `/gsd-audit-fix` is relaunched as plain execute-phase.
**How to avoid:** Pattern 3. Same latent gap on the cron arm today.

### Pitfall 4: A "small" core change is a semver-major break
**What goes wrong:** `cargo semver-checks` fails the 3.1.0 minor.
**Verified this session with `cargo-semver-checks 0.51.0` on a scratch crate** (probe deleted):
- Adding `#[serde(default)] pub terminal_reason: Option<String>` to an all-`pub`, non-`#[non_exhaustive]`
  struct → `failure constructible_struct_adds_field: struct exhaustively constructible through public API adds field … Summary semver requires new major version: 1 major and 0 minor checks failed`.
- Adding the same field as **private** → `failure constructible_struct_adds_private_field … semver requires new major version`.
- Control (same run): adding a field to a `#[non_exhaustive]` struct and adding serde derives to a
  `#[non_exhaustive]` enum → no failure (only the one lint above failed out of 229).
`IdleTimeoutRecord` is `#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)] pub struct IdleTimeoutRecord { pub status: String, pub idle_secs: u64, pub agent_pid: u32, pub written_at: u64, pub commits: Vec<IdleTimeoutCommit> }`
with no `#[non_exhaustive]` `[VERIFIED: crates/devflow-core/src/agent_result.rs:2272-2289]`, in
`pub mod agent_result` (`lib.rs:55`). So **D-15's "new optional fields" cannot go on the pub Rust
struct in a minor release.**
**How to avoid (recommended):** leave `IdleTimeoutRecord` untouched; write the on-disk record from a
crate-private struct `{ #[serde(flatten)] record: IdleTimeoutRecord, #[serde(default)] terminal_reason, errors, origin_kind }`
and read it back through the same private struct. `IdleTimeoutRecord` has no
`deny_unknown_fields`, so an older reader ignores the new keys and a pre-51 file parses with the
new fields defaulted. This satisfies D-15's on-disk contract (optional `#[serde(default)]` fields in
the record) without a major bump. Same rule for `StageIntent` (`prompt.rs:165-184`, exhaustive pub
enum) and `Action`. `State` is `#[non_exhaustive]` `[VERIFIED: crates/devflow-core/src/state.rs:34-36]`,
so new fields there are additive. Keep new public fns additive; do not change the signature of
`pub fn capture_shows_rate_limit_denial(project_root: &Path, phase: PhaseId) -> bool`
(`agent_result.rs:2357`) — compute the torn signal inside it.
**Warning signs:** run `cargo semver-checks check-release -p devflow-core` per plan that touches core.

### Pitfall 5: Torn final line hides a rate limit in the idle-timeout path
**What goes wrong:** `capture_shows_rate_limit_denial` calls
`detect_claude_stream_rate_limit(&ParsedCapture::parse(&raw).events).is_some()`
`[VERIFIED: agent_result.rs:2357-2362]`; a torn final `rate_limit_event` is not in `events`, so the
monitor writes an idle-timeout record ("TERMINAL and not retried automatically") instead of
suppressing it. The read path is already safe in the other direction: `parse_claude_event_result`
returns `indeterminate_capture_failure()` ("stream capture ends in an unparseable line; the final
verdict is indeterminate") when a torn JSON line follows the last top-level result, before the
rate-limit check `[VERIFIED: agent_result.rs:1876-1895, 2159-2173]`.
**How to avoid:** make the stream check return a tri-state internally (denied / not denied / torn
after the last parsed event) and have the monitor's idle path treat "torn" as "do not claim a clean
silence": e.g. write the record but say a torn final line was present, or skip the record so the
read path's indeterminate verdict gates. Never classify a torn line as `RateLimited` (no proof; and
a substring match would let agent-authored text drive the resume schedule, T-30-12). The single
denial status is `const CLAUDE_STREAM_RATE_LIMIT_DENIAL_STATUSES: &[&str] = &["rejected"];`
`[VERIFIED: agent_result.rs:1692]`.
**Where tears really come from:** the pipe-owning reader tees whole lines with
`writeln!(file, "{line}")` then `flush` `[VERIFIED: monitor.rs:1224-1232]`, after `read_until(b'\n')`
— a child killed mid-line yields a partial line at EOF that is teed with a newline, i.e. a
permanent torn line. The capture shape hands the file to the agent directly (`spawn_stage_agent`,
`monitor.rs:2158-2175`).

### Pitfall 6: Relaunch failure after the commit leaves a "handed off, nothing running" zombie
**What goes wrong:** `run_monitor` returns the advance's `Err` (other than `NotOwner`) and exits
`[VERIFIED: pipeline_launch.rs:1632-1645]`; no gate, no cron, `state.agent` already switched.
**How to avoid:** Pattern 2's `Err` arm; test with the `launch_context()` seam.

### Pitfall 7: The WR-13 test cannot reuse `resume_with` as written
**What goes wrong:** `resume_with` polls until the saved command differs from `"old"`
`[VERIFIED: crates/devflow-cli/tests/gate_notify_persist_e2e.rs:362-406]`; with the variable unset
it never differs, so the helper would panic on its 20 s timeout.
**How to avoid:** a separate helper that waits for a progress marker strictly after
`commit_notify_env` (e.g. `stopped == false` with a `pending_supervisor`/`supervisor` recorded, or
the fake agent's marker), then asserts `Some("old")`. Mutation control: changing `commit_notify_env`'s
`apply_notify_env(state.gate_notify_cmd.clone())` to `apply_notify_env(None)`
(`pipeline_launch.rs:1200-1206`) must fail it.

### Pitfall 8: e2e timing on the stream path
**What goes wrong:** a stream-shape rate limit typically reaches advance only after the idle
timeout, whose floor is `pub const IDLE_TIMEOUT_FLOOR_SECS: u64 = 120;`
(`DEVFLOW_CLAUDE_IDLE_TIMEOUT_SECS` can only raise it) `[VERIFIED: monitor.rs:133, 147]`.
**How to avoid:** drive D-11's e2e on the capture shape (`state.legacy_claude_launch = true`, as
the 50-13 harness does) with a single-document envelope
`{"type":"result","subtype":"error_rate_limit","is_error":true,"retry_after":"<ISO-8601>"}`; the
stream classification is covered by existing monitor/agent_result tests. State the limit: the e2e
proves the AutoResume branch, not the stream-specific route into it.

## Code Examples

### Cron arm output contract (unchanged)
```rust
// Source: crates/devflow-core/src/ship.rs:270-300 [VERIFIED]
// resume.command = "devflow", resume.args = ["resume", "--phase", phase]
// hermes_cron.command = format!("cd {project} && devflow resume --phase {phase}")
// hermes_cron.schedule = hermes_schedule_from_retry_after(retry_after).unwrap_or_default()
```
`retry_after_from_reason` strips the `"rate limited until "` prefix, else `"unknown"`
`[VERIFIED: crates/devflow-cli/src/parallel.rs:145-150]`; an empty schedule routes to
`gate_or_abort_infra` (never silent) `[VERIFIED: pipeline_outcomes.rs:117-126]`.

### Notify command resolution (999.154)
```rust
// Source: crates/devflow-cli/src/pipeline_launch.rs:1200-1206 [VERIFIED]
fn commit_notify_env(state: &mut State) -> Result<(), CliError> {
    let resolved = devflow_core::gates::apply_notify_env(state.gate_notify_cmd.clone());
    if resolved == state.gate_notify_cmd { return Ok(()); }
    commit(state, |s| s.gate_notify_cmd = resolved)
}
```
Gate entry is `commit(state, |s| s.gate_pending = true)?;` (`pipeline_gate.rs:530`), and `commit`
goes through `update_state`/`update_owned_state`, which reload from disk and return `Err` on an
`Existing::Unloadable` file (`workflow.rs:539-551`). So WR-12 (stale copy reverts an on-disk edit)
and IN-10 (unparseable file silently rewritten) are both **likely** no longer reproducible
`[ASSUMED — must be reproduced by test per D-17]`. Existing neighbours:
`pipeline_gate::tests::gate_resolution_keeps_a_notify_cmd_edited_during_the_wait` (`pipeline_gate.rs:2604`),
`pipeline_launch::tests::advance_commits_a_changed_notify_command_where_it_sets_it` (`pipeline_launch.rs:14254`).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `sequentagent`: second worktree + branch, `git.rebase_in`, manual conflict resolution | removed; `resume --agent` same-worktree substitution | 23-07/23-08 (`dbab01a`, `0833a6c`); v2.8.0 for `resume --agent` | Failover reuses the substitution; criterion 4 keeps the old shape dead |
| `write_rate_limit_cron` (parallel.rs) | `handle_rate_limited_outcome` → `build_single_agent_cron_instructions` | `dbab01a` | Criterion 2's name refers to the live cron arm |
| `claude_stream_events` → bare `Vec<Value>` | `ParsedCapture { events, line_shapes }` with `TornJson` | Phase 30 | 999.71's premise partly stale (D-16) |
| Shell `sh -c` monitor, `advance` as a separate process | Rust `__monitor` supervisor, advance in-process, socket liveness | Phases 31, 50, 50.1 | Failover relaunch runs in the supervisor; no foreground reservation |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | A git-cleanliness test of `*-PLAN.md` files is a sound "plan complete" signal (GSD plan-phase commits plans at its end) | Pattern 3 | A complete-but-uncommitted plan set is redone by the fallback (wasteful, not unsafe); a committed partial set short-circuits (unsafe). The D-09 tests bound it; confirm against GSD's commit behaviour |
| A2 | WR-12 and IN-10 no longer reproduce because `commit` reloads and refuses an unloadable file | Code Examples | If they do reproduce, D-17 requires fixing them in this phase |
| A3 | Claude's CLI usually stays alive after a stream-path rate limit, so that path reaches advance via the idle timeout | Pitfall 8 | Only affects how long a real failover takes, not correctness |
| A4 | The ROADMAP's "`n` verb" names nothing that ever existed (no commit matches) — likely a stand-in for `sequentagent` | Pattern 5 | None for the scan (D-14 covers both readings); worth a one-line note in the verification evidence |
| A5 | Clearing `session_id` at handoff is harmless | Pitfall 1 | Low; checkpoint resume is Claude-only |

## Open Questions

1. **Does the Plan/Code continuation also apply to the cron arm's `resume`?**
   - Known: the same two gaps exist on that arm today.
   - Unclear: whether fixing it is in SUPV-02's scope.
   - Recommendation: apply the persisted fix type to every relaunch (it is the same field); for the
     Plan prompt, apply it on failover only and file a `999.x` entry for the cron arm unless the
     plan makes it free.
2. **Which arm after a post-commit relaunch failure — cron or gate?**
   - D-04 allows either. Recommendation: the cron arm (`handle_rate_limited_outcome`), because the
     limit still holds for the primary and the saved agent is the fallback; it already gates on an
     unparseable retry hint.
3. **999.71 measurement deliverable shape.** Preliminary data gathered this session (see below).
   Recommendation: an `events.jsonl` record of the capture's final-line shape at each evaluation
   (cheap, continuous, read at the moment DevFlow reads) plus an archived measurements file.

**Preliminary 999.71 measurement (this session, read-only):** a probe classified the final
non-empty line of every archived capture under the main checkout's `.devflow/history/`
(phases 43, 51, 900): `summary: files=18 parsed=18 torn=0 noise=0 empty=0`; a planted capture
ending `{"type":"rate_limit_ev` (no newline) was reported `TORN` (negative control). **What this
does not establish:** these are captures read after the writer finished (archived at the next
launch), not at the moment advance reads them; 18 captures from 3 phases is a weak bound (zero of
18 is consistent with a tear rate up to roughly 15%); no sample is known to come from a killed
agent, which is the realistic tear source. It is not the D-16 harness.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo / rustc | everything | ✓ | cargo 1.97.1 | — |
| cargo-nextest | named-test gates | ✓ | 0.9.150 | — |
| cargo-semver-checks | core pub changes | ✓ | 0.51.0 | — |
| cargo-mutants | optional mutation check | ✓ | 27.1.0 | skip |
| cargo-llvm-cov | optional coverage | ✓ | 0.9.1 | skip |
| git, sh, sleep | e2e fixtures | ✓ | system | — |
| Real `claude`/`codex` | not needed (stub agents) | — | — | stubs on an agent-free PATH |

**Missing dependencies with no fallback:** none.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness, run via cargo-nextest 0.9.150 |
| Config file | none required (CLAUDE.md gate form) |
| Quick run command | `cargo nextest run -p <pkg> (--lib \| --bin devflow) --no-tests=fail -E 'test(=<module::path::name>)'` |
| Full suite command | `scripts/check.sh all` (fmt + clippy `-D warnings` + `cargo test --workspace --no-fail-fast` with insta guards) |
| Semver gate | `cargo semver-checks check-release -p devflow-core` |

Notation: CORE = `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=X)'`;
CLI = `cargo nextest run -p devflow --bin devflow --no-tests=fail -E 'test(=X)'`;
E2E = `cargo nextest run -p devflow --test <file> --no-tests=fail -E 'test(=X)'`.
Proposed names are behaviour-led; the planner may rename but must keep one test per gate.

### Phase Requirements → Test Map
| Req / Criterion / Decision | Behavior | Type | Automated Command (X =) | File Exists? |
|-----|----------|------|-------------------|-------------|
| SUPV-02 crit 1, D-11 (fallback arm) | Claude stub emits rate-limit envelope; fallback stub completes the same stage in the same worktree/branch; `agent_handoff` with `reason: rate_limited`; **control:** no cron file, no `rate_limit_resume_scheduled` | e2e (full monitor harness) | CLI `pipeline_launch::tests::a_rate_limit_with_a_fallback_hands_off_and_completes_the_same_stage` | ❌ Wave 0 |
| crit 2, D-11 (no-fallback arm) | Same rig, no fallback: cron instructions written, `rate_limit_resume_scheduled` emitted; **control:** no `agent_handoff`, agent unchanged | e2e | CLI `pipeline_launch::tests::a_rate_limit_without_a_fallback_writes_cron_instructions_and_never_hands_off` | ❌ Wave 0 |
| crit 2 (existing cron arm) | ceiling and unparseable-hint behaviour kept | unit | CLI `pipeline_outcomes::tests::rate_limited_at_infra_ceiling_stops_resuming_and_aborts` | ✅ |
| D-05 | A rate limit after the hop (fallback consumed) takes the cron arm, run stays on the fallback | unit/integration | CLI `pipeline_launch::tests::a_rate_limit_after_the_one_hop_takes_the_cron_arm` | ❌ |
| D-06 | `infra_failures = MAX_INFRA_FAILURES - 1` + unused fallback → hands off, counter unchanged; **control:** same state without fallback aborts | unit | CLI `pipeline_launch::tests::an_unused_fallback_hands_off_one_below_the_infra_ceiling` | ❌ |
| D-07 | Fallback binary missing at handoff → `agent_handoff_refused` event naming the reason + cron arm; agent unchanged | unit (agent-free PATH child) | CLI `pipeline_launch::tests::a_refused_handoff_takes_the_cron_arm_and_names_the_refusal` | ❌ |
| D-04 canary | Recorded Claude `Confirmed`, handoff to Antigravity, canary closure returns `Absent` → launch refused; **control:** without invalidation the recorded `Confirmed` admits it | unit | CLI `pipeline_launch::tests::a_handoff_reruns_the_delivery_canary_for_the_new_agent` | ❌ |
| D-04 post-commit failure | Relaunch fails after the commit (no `DEVFLOW_CACHE_DIR`) → event + cron arm or gate; never "handed off, nothing running" | unit (child) | CLI `pipeline_launch::tests::a_handoff_whose_relaunch_fails_ends_in_the_cron_arm` | ❌ |
| D-04 shared fn | `resume --agent` still relaunches with reason `resume --agent` | unit | CLI `pipeline_launch::tests::resume_with_agent_from_a_rate_limited_state_relaunches` | ✅ (keep green) |
| D-04 failure path of resume | resume re-marks stopped on launch failure | unit | CLI `pipeline_launch::tests::resume_re_marks_stopped_when_launch_stage_fails_outright` | ✅ (keep green) |
| D-01 state | fallback fields round-trip; a pre-51 state loads with them defaulted | unit | CORE `state::tests::fallback_fields_round_trip_and_tolerate_states_predating_them` | ❌ |
| D-01 resume | `resume --fallback-agent` sets it and clears the consumed mark; refused over a live run (existing guard) | unit | CLI `pipeline_launch::tests::resume_with_a_fallback_agent_rearms_a_consumed_fallback` | ❌ |
| D-02 | `start`/`resume` refuse fallback == agent, message names both | unit | CLI `commands::tests::start_refuses_a_fallback_equal_to_the_agent` | ❌ |
| D-03 | `--mode auto` refuses a codex fallback at start; **control:** antigravity accepted | unit | CLI `commands::tests::start_in_auto_mode_refuses_a_fallback_outside_the_unattended_launch_shape` | ❌ |
| D-09 Plan | uncommitted partial `PLAN.md` → continuation prompt (no short-circuit text); **control:** committed clean plan set → default prompt | unit | CLI `pipeline_launch::tests::a_plan_failover_over_an_uncommitted_plan_does_not_short_circuit` | ❌ |
| D-09 Code | failover during audit-fix pass renders `/gsd-audit-fix` (Claude) / audit-fix workflow (Codex); **control:** plain Code renders execute-phase | unit | CLI `pipeline_launch::tests::a_code_failover_during_an_audit_fix_pass_replays_audit_fix` | ❌ |
| D-10 | `agent_handoff` carries tree dirty flag and commit count; dirty tree is not cleaned | unit | CLI `pipeline_launch::tests::the_handoff_event_records_the_tree_it_hands_over` | ❌ |
| crit 3, D-12 | baseline / withheld publish probe on failover / withheld probe on an ordinary transition (control) | e2e | CLI `pipeline_launch::tests::failover_and_an_ordinary_launch_fail_alike_without_the_publish_probe` (+ trace recorded in VERIFICATION) | ❌ |
| crit 4, D-14 | no `rebase_in` ident, no `Sequentagent` variant, no `n` name/alias in comment-stripped source; planted positives trip it, comment decoys and `ship.rs`'s literal do not | source scan | CORE `doc_check::the_removed_sequentagent_handoff_stays_dead` | ❌ |
| D-15 | cause fields present → named in reason; absent → today's message | unit | CORE `agent_result::tests::an_idle_timeout_reason_names_the_last_result_cause` and `agent_result::tests::an_idle_timeout_without_cause_fields_keeps_todays_message` | ❌ |
| D-15 back-compat | a pre-51 record (no new keys) parses, not "unreadable" | unit | CORE `agent_result::tests::a_pre_51_idle_timeout_record_still_parses` | ❌ |
| D-15 write | monitor writes cause fields from the last `result` event | unit | CORE `monitor::tests::the_idle_timeout_record_carries_the_last_result_cause` | ❌ |
| D-16 | torn final `rate_limit_event` is not read as a clean non-rate-limited ending; **control:** intact denial still detected | unit | CORE `agent_result::tests::a_torn_final_line_is_not_read_as_a_clean_silence` | ❌ |
| D-17 WR-13 | `resume` with variable unset keeps the saved command; mutation to `apply_notify_env(None)` fails it | e2e | E2E (`--test gate_notify_persist_e2e`) `resume_with_the_variable_unset_keeps_the_saved_command` | ❌ |
| D-17 WR-12 | reproduction: on-disk notify edit after advance loaded survives gate entry and is the command fired | unit | CLI `pipeline_gate::tests::gate_entry_fires_a_notify_cmd_edited_after_the_advance_loaded` | ❌ |
| D-17 IN-10 | reproduction: unparseable state at gate entry is not rewritten (bytes unchanged) and is reported | unit | CLI `pipeline_gate::tests::gate_entry_over_an_unparseable_state_does_not_rewrite_it` | ❌ |
| semver | devflow-core stays minor-compatible | tool | `cargo semver-checks check-release -p devflow-core` | n/a |

**What these tests do not establish (state in VERIFICATION):** the e2e uses stub agents and the
capture shape; it does not prove a real provider's rate-limit envelope still matches the
classifier, nor the stream path's timing. Criterion 1 under a real limit is D-13's optional
dogfood evidence only.

### Sampling Rate
- **Per task commit:** the task's named CORE/CLI gate(s), one per command, chained with `&&`.
- **Per wave merge:** `cargo test -p devflow-core --lib && cargo test -p devflow --bin devflow`, plus
  `cargo semver-checks check-release -p devflow-core` if the wave touched core.
- **Phase gate:** `scripts/check.sh all` green, then `scripts/check-in-container.sh`, before
  `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] A rate-limit stub for the 50-13 handoff harness: a fake `claude` whose first run prints the
      single-document rate-limit envelope and exits, plus a fake fallback binary (e.g. `codex`)
      that prints `DEVFLOW_RESULT: {"status":"success"}`; reuse `handoff_claude_dir` /
      `run_handoff_child` (`pipeline_launch.rs:9137-9178`).
- [ ] A `#[cfg(test)]` publish-probe withholding seam + its `doc-check-allowlist.toml` entry (D-12).
- [ ] A WR-13 helper in `gate_notify_persist_e2e.rs` that waits on a post-`commit_notify_env` marker.
- No framework install needed.

## Security Domain

`security_enforcement` is not set to false in `.planning/config.json`, so this section applies.
No authentication, sessions, access control or cryptography is touched.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes | clap `ValueEnum` parsing of `--fallback-agent` into `AgentKind`; typed serde reads of capture fields (`Value::get`, never `json_find_key` on agent-authored content) |
| V6 Cryptography | no | — |
| V7 Error/Logging | yes | never-silent refusal events; bound the length of `errors[]` text placed in reasons/gate context |

### Known Threat Patterns for this phase
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Agent-authored text forges a rate limit or retry time (T-30-12) | Spoofing/Tampering | Keep `detect_claude_stream_rate_limit`'s direct top-level `Value::get` reads; never classify a torn line by substring |
| Idle-timeout cause fields carry multi-KB or control characters into gate context and the notify hook env | Tampering/DoS | Truncate and sanitise before use; the hook already passes context as env vars, not shell-interpolated (`gates.rs` notify doc) |
| A handoff leaves the run "active" with nothing running | Denial of service (silent stall) | Pattern 2 `Err` arm → cron arm or gate |
| Fallback skips a guard the primary passed (canary) | Elevation (unverified launch) | Clear `canary` in the handoff commit |
| Notify command (may carry a webhook token) reaching the agent env | Information disclosure | Unchanged: `strip_notify_env` on every spawn (`monitor.rs:392-394`) |

## Sources

### Primary (HIGH confidence) — read this session
- `crates/devflow-cli/src/pipeline_launch.rs` (launch_stage 2856-2889, handle_ambiguous_outcome 2901-2935, resume/resume_loaded 2975-3297, AutoResume arm 3736-3741, run_monitor 1512-1680, spawn_next_supervisor 2489-2580, canary 542-576, stream_launch_enabled 825-829, commit_notify_env 1200-1206, launch_context 1958-2010, 50-13 harness 9120-9240, finding-2a test 5621-5672)
- `crates/devflow-cli/src/pipeline_outcomes.rs` 40-157; `pipeline_gate.rs` 160-219, 513-600
- `crates/devflow-cli/src/preflight.rs` 86-99, 965-1060, 1258-1290; `commands.rs` 346-420, 927-1000, 4601-4628; `main.rs` 47-117, 186-206, 570-662
- `crates/devflow-core/src/agent_result.rs` 85-160, 240-300, 1200-1330, 1692-1900, 2253-2440, 2440-2500; `monitor.rs` 125-148, 380-394, 1224-1400, 2150-2175, 2680-2890, 2429-2470; `outcome_policy.rs` 12-50; `prompt.rs` 160-260, 380-420, 474-535; `state.rs` 33-44, 380-410, 425-450; `ship.rs` 265-300, 910-945; `gates.rs` 1490-1540; `doc_check.rs` 1-130, 185-225, 405-450, 500-600; `supervisor.rs` 340-360
- `crates/devflow-cli/tests/gate_notify_persist_e2e.rs` 1-30, 355-436
- `cargo semver-checks 0.51.0` scratch probe (output quoted in Pitfall 4)
- git history: `dbab01a`, `0833a6c` (removed `Sequentagent`, `write_rate_limit_cron`, `rebase_in`)
- GitHub issue #180 (`gh issue view 180`)
- `.planning/ROADMAP.md` Phase 51, 999.102, 999.71, 999.154; `.planning/REQUIREMENTS.md` SUPV-01/02

### Secondary / Tertiary
- None. No web or library-doc research was needed: the phase adds no external dependency.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; tool versions probed.
- Architecture: HIGH for call paths (read); MEDIUM for the recommended mechanisms (untested designs).
- Pitfalls: HIGH for canary, semver, torn-line, zombie and WR-13 helper (read or probed); MEDIUM for
  the stream-path timing (A3).

**Research date:** 2026-11-05
**Valid until:** until the next change to `pipeline_launch.rs`'s advance/launch path (fast-moving
file); re-check line numbers before citing them in PLAN.md.
