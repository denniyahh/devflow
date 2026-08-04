# Phase 28: Close the Checkpoint Answer Return Path - Research

**Researched:** 2026-07-30
**Domain:** DevFlow's own Rust source (`devflow-core`, `devflow-cli`) — agent process
lifecycle, gate/state persistence, and the Claude Code CLI's headless `--resume`
feature. No new external dependency, no new library.
**Confidence:** HIGH — every canonical ref cited in `28-CONTEXT.md` was re-read
against live source this session; all cited claims confirmed, one previously
undocumented gotcha found (see Pitfall 1).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Checkpoint classification**
- **D-01:** DevFlow statically scans the stage's `PLAN.md`(s) for any task
  carrying `gate="blocking-human"` **before** launching the stage. If none
  exist, any non-success exit is an ordinary error — exactly today's
  behavior, unchanged. If one or more exist, a non-success exit is
  *possibly* that checkpoint, and DevFlow confirms which by reading the
  `Gate:` field GSD's own `checkpoint_return_format` already produces in
  the executor's structured return (`checkpoints.md`). No new agent-side
  contract is invented — DevFlow reads a field the executor already emits
  when a checkpoint fires.
  — Reversibility: reversible.
- **D-02:** Rejected: pattern-matching the failure reason's text for
  checkpoint-shaped content (same failure class as the tag-signing-viability
  predictor rejected twice in Phase 26, `26-CONTEXT.md` D-10).

**Checkpoint resolution — unconditional autonomous decide**
- **D-03:** DevFlow's default, unconditional behavior for a
  `gate="blocking-human"` checkpoint is to have the agent decide it itself.
  No flag or config toggle. This is a deliberate, informed override of
  `checkpoints.md` rule 6, applied unconditionally rather than as an
  auto-mode exception, after two rounds of explicit operator pushback
  (including the Phase 26 near-miss where a mistagged `gate="blocking"`
  would have silently authorized `cargo publish`).
  — Reversibility: costly — a policy reversal, and any autonomous decision
  executed while this policy was live is a real-world action code cannot
  undo.
- **D-04:** Mechanism: on a confirmed checkpoint, DevFlow relaunches via
  `claude -p --resume "$session_id" "<synthesized instruction>"` — the same
  exited session, not a fresh process. `session_id` is captured from the
  JSON envelope DevFlow already receives (`--output-format json`;
  `agent_result.rs`'s `parse_devflow_result` currently reads only the inner
  `result` text and discards the rest of the envelope). Persist it on
  `State`, same shape as `worktree_path`/`monitor_pid`. Directory-scope
  (`--resume` requires invoking from the session's original working
  directory) is already satisfied by `spawn_monitor` always launching from
  `state.worktree_path`, consistently, across relaunches.
  — Reversibility: reversible — additive field + a Claude-specific relaunch
  code path.
- **D-05:** Claude-only. No `AgentAdapter` trait change, no Codex/OpenCode
  accommodation. `--resume` is Claude-CLI-specific; a cross-agent fallback
  ("Part B," a structured answer file) was considered and explicitly
  declined for now.
  — Reversibility: reversible.
- **D-06:** Rejected: an explicit `--auto-decide-checkpoints` opt-in flag,
  and consolidating with `yes_ship` into one umbrella flag (different risk
  shapes — Ship approval is one coarse decision, checkpoint content is
  arbitrary and planner-judged). Superseded by D-03.
- **D-07:** DevFlow still records what was auto-decided (D-04's mechanism),
  even though nothing blocks on it and no config disables it. Mirrors Phase
  23's D-06 principle: "the audit trail must show a decision, never a
  missing checkpoint" — now the *only* way anyone learns after the fact what
  the agent decided on its own.
  — Reversibility: reversible.

**Why not build a human-answer path (recorded for the record)**
- **D-08:** No built-in, zero-config notification. `fire_gate_notify`/
  `DEVFLOW_GATE_NOTIFY_CMD` is operator-supplied; unset, silent no-op.
- **D-09:** `Gates::poll_response` is a real blocking loop with a 7-day
  production timeout — the documented cause of DevFlow's "gates hang
  forever" failure class.
- **D-10:** Considered and rejected: routing checkpoint pauses through
  `stopped`/`stop_reason`/`resume` instead — still fundamentally
  human-in-the-loop, does not meet the actual goal. Superseded by D-03.
- **D-11:** A human-answer path for checkpoints is explicitly **not built
  this phase** — deferred to a future phase that builds real
  notification/response infrastructure first.

**Ship approval (`yes_ship`)**
- **D-12:** `yes_ship` gains a persistent config option (settable in
  `devflow.toml`), in addition to its existing per-invocation `--yes-ship`
  CLI flag; the CLI flag still overrides (in practice: ORs with) the config
  value. Deliberate reversal of Phase 23's D-05 ("`--yes-ship` is a per-run
  flag only — never config-persistable").
  — Reversibility: costly.
- **D-13:** Phase 23's D-06 (auto-answer the Ship gate, don't bypass it) is
  unaffected — D-12 only changes where `yes_ship`'s value can come from.

**Define stage headless safety**
- **D-14:** When `CONTEXT.md` doesn't exist, Define must never invoke
  `/gsd-discuss-phase` headlessly. `Stage::Define.gsd_command()` currently
  returns `/gsd-discuss-phase {N}` unconditionally in the "artifact does not
  exist" branch of `idempotent_stage_prompt` (`prompt.rs`). Fix is deletion,
  not disambiguation: proceed without `CONTEXT.md` (same as any other early
  phase with no context file).
  — Reversibility: reversible.

**`--until` cap preservation**
- **D-15:** `resume` clears `stopped`/`stop_reason`/`stop_until`
  unconditionally (`pipeline_launch.rs:226-228`), silently discarding a
  `--until` cap that never fired. Gate the clear on `state.stopped`.
  — Reversibility: reversible.

### Claude's Discretion

Not constrained by CONTEXT.md; resolved with concrete recommendations below
(see `## Discretion Resolutions`):
- Exact wording/shape of the synthesized "no operator available, use your
  own judgment" instruction relayed via `--resume` (D-04).
- Exact shape/location of the audit record (D-07).
- Exact config key name/shape for `yes_ship`'s new config option (D-12) and
  where in `devflow-core::config` it's parsed.
- Where the static `PLAN.md` scan (D-01) lives and how it locates the
  relevant plan file(s) for the current stage/phase.
- Mechanical shape of `session_id` persistence on `State` (D-04).

### Deferred Ideas (OUT OF SCOPE)

- Human-answer path for checkpoints (a dedicated `devflow gate answer` verb,
  or a resume-relaunch carrying a human-typed answer) — needs real
  notification/response infrastructure first.
- The notification/response interface itself.
- Ship-gate redundancy (`yes_ship` already expresses intent, yet DevFlow
  still always writes and blocks on a Ship gate when `yes_ship` is false) —
  raised, explicitly left unbuilt.
- Cross-agent (Codex/OpenCode) checkpoint resolution — D-05 scopes this
  phase to Claude only.
</user_constraints>

<phase_requirements>
## Phase Requirements

No REQ-IDs exist for this project (tracked by backlog identifier, consistent
with Phases 21/22/26/27).

| Backlog ID | Description | Research Support |
|------------|-------------|-------------------|
| 999.57 / DEN-82 | Close the checkpoint answer return path (primary) — 28a/28b | Confirmed exact insertion points for D-01's scan, D-04's `session_id` capture + `--resume` relaunch, and D-07's audit event. See Architecture Patterns and Code Examples. |
| 999.59 / DEN-84 | Define headless safety when CONTEXT.md is missing — 28c | Confirmed exact branch to delete in `prompt.rs::idempotent_stage_prompt` (D-14). |
| 999.60 / DEN-85 | `resume` must not clear an unfired `--until` cap — 28d | Confirmed exact 3-line unguarded clear at `pipeline_launch.rs:226-228` (D-15), line numbers unchanged from CONTEXT.md's citation. |
| (D-12/D-13) | `yes_ship` persistent config option | Confirmed the exact sibling pattern to extend (`external_verify_enabled`), and the exact test whose assertion must flip (`config_file_with_yes_ship_key_loads_but_never_sets_the_flag`, `pipeline_outcomes.rs:1631`). |
</phase_requirements>

## Summary

This phase is source-only — no new crate, no new external dependency. Every
canonical file CONTEXT.md cited was re-read against live source this session;
**nothing has drifted**. `pipeline_launch.rs:226-228`'s unguarded
`stopped`/`stop_reason`/`stop_until` clear is at the exact lines CONTEXT.md
named. `agent_result.rs:1362` is exactly the fixture CONTEXT.md quoted as
ground truth for the envelope shape. The four units are small, mechanical,
additive changes layered onto code that already has the right shape:
`AgentResult` already parses the Claude JSON envelope (just not `session_id`
yet); `State` already has four `#[serde(default)]` fields with the exact
backward-compat pattern `session_id` needs; `DevflowConfig` already has an
`external_verify_enabled: bool` resolved with env-var-overrides-file
precedence that `yes_ship`'s new config option should copy verbatim;
`verify::external_verify_commands` already implements the exact
`.planning/phases/{NN}-*/{NN}-*-PLAN.md` discovery loop D-01's static scan
needs (a different concern, but literally reusable file-discovery logic);
and `events::emit()` already writes an append-only `.devflow/events.jsonl`
that is the pre-existing, load-bearing home for D-07's audit record — no new
file, no new subsystem.

One thing CONTEXT.md's discussion did **not** surface and this research did:
Claude Code's own docs state that `--resume` does **not** restore permission
mode — `bypassPermissions` (what `--dangerously-skip-permissions` sets) "must
be enabled again at launch." DevFlow's `ClaudeAgent::exec_command` always
passes `--dangerously-skip-permissions`; the D-04 relaunch path must pass it
again on the `--resume` invocation or the resumed session will stall waiting
for a permission prompt nobody is present to answer — the exact headless
hazard this whole phase exists to close, reintroduced by omission. See
Pitfall 1.

**Primary recommendation:** implement all four units as small, additive
changes to existing structures (new `State` field, new `DevflowConfig`
field, one new pure function in `verify.rs`, one new relaunch code path in
`agents/claude.rs`, one new event kind via the existing `events::emit()`),
touching no external dependency and no shared/cross-agent trait surface.

## Architectural Responsibility Map

DevFlow is a single-process Rust CLI (`devflow-core` library +
`devflow-cli` binary) that orchestrates external coding-agent CLIs as
subprocesses. There is no browser/frontend/API tier — the relevant tiers are
process-orchestration layers within one codebase.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Checkpoint recognition (D-01) | `devflow-core::verify` (new pure fn, PLAN.md scan) | `devflow-cli::pipeline_launch::advance` (dispatch site) | Discovery of declared PLAN.md metadata is a `devflow-core` concern (mirrors `external_verify_commands`); the dispatch decision belongs to the CLI's pipeline orchestration, same split as the existing Layer 0 verification path. |
| Checkpoint confirmation (D-01, reading `Gate:`) | `devflow-core::agent_result` (stdout parsing) | — | Stdout parsing is already centralized here (`parse_devflow_result`, `detect_claude_envelope_failure`); a `Gate:` field reader is a sibling function in the same module. |
| Session-id capture (D-04) | `devflow-core::agent_result` (`AgentResult`/`parse_devflow_result`) | `devflow-core::state` (`State` persistence) | The envelope is already parsed here; persistence already has the exact field-shape precedent (`monitor_pid`, `worktree_path`). |
| `--resume` relaunch (D-04/D-05) | `devflow-core::agents::claude` (Claude-only adapter) | `devflow-cli::pipeline_launch` (call site) | D-05 locks this to the Claude adapter — no `AgentAdapter` trait change. The call site that decides *when* to relaunch (vs. ordinary `launch_stage`) is pipeline orchestration in the CLI crate. |
| Auto-decide audit record (D-07) | `devflow-core::events` (existing `emit()`) | — | Pre-existing, single home for every workflow event; no new subsystem needed. |
| `yes_ship` config option (D-12) | `devflow-core::config` (`DevflowConfig`) | `devflow-cli::commands::start` (OR-combine with CLI flag) | Exact sibling of `external_verify_enabled`'s existing resolution chain (env var > `devflow.toml` > built-in default). |
| Define headless-safety fix (D-14) | `devflow-core::prompt` (`idempotent_stage_prompt`) | — | Prompt construction is entirely local to this module; no other tier is involved. |
| `--until` cap preservation (D-15) | `devflow-cli::pipeline_launch::resume` | — | The unguarded clear is a 3-line, single-function fix; no other tier participates. |

## Standard Stack

Not applicable — this phase adds no new external dependency. Every change is
additive Rust source inside the existing `devflow-core`/`devflow-cli`
workspace, using crates already in the dependency graph (`serde`,
`serde_json`, `toml`, `thiserror`, `tracing` — see
`crates/devflow-core/Cargo.toml`). No `cargo add` is required by any of the
four units.

## Package Legitimacy Audit

Not applicable — no external packages are installed by this phase. Skip the
Package Legitimacy Gate protocol entirely; there is nothing to audit.

## Architecture Patterns

### System Architecture Diagram

```
 devflow start --phase N
        │
        ▼
 commands::start()  ──(D-12 site)──▶ state.yes_ship = cli_flag || config::yes_ship(root)
        │
        ▼
 pipeline_launch::launch_stage
        │  spawns monitor::spawn_monitor(state, "claude", ["-p", prompt,
        │      "--output-format","json","--dangerously-skip-permissions"])
        ▼
 [detached monitor process — owns the agent]
   sh -c '"$@" > stdout 2>stderr & wait; echo $? > exit; devflow advance --phase N'
        │
        ▼  (agent process exits; monitor reaps exit code, calls `devflow advance`)
 pipeline_launch::advance(project_root, phase)
        │
        ▼
 agent_result::evaluate_agent_result()  (Layer 0→1→2→3 cascade)
   Layer 1 already parses the Claude JSON envelope for is_error/rate-limit/
   DEVFLOW_RESULT marker — D-04 adds: capture `session_id` into AgentResult
        │
        ▼
 outcome_policy::decide_action(stage, result.status)
        │
        ├─ Action::Advance ─────────────▶ transition() → next stage
        │
        └─ Action::GateReview ──▶ [NEW — D-01 insertion point, before the
                                    existing dispatch below]
                                    verify::phase_has_blocking_human_checkpoint(
                                        project_root, phase)?
                                      │
                                      ├─ false ─▶ handle_stage_failure()
                                      │            (today's unchanged path:
                                      │             never-silent generic gate)
                                      │
                                      └─ true  ─▶ scan captured stdout for
                                                   "**Gate:** blocking-human"
                                                   (checkpoints.md's
                                                   checkpoint_return_format)
                                                     │
                                                     ├─ confirmed ─▶ [NEW —
                                                     │   D-03/D-04 path]
                                                     │   synthesize instruction
                                                     │   → claude -p --resume
                                                     │   "$session_id"
                                                     │   --output-format json
                                                     │   --dangerously-skip-
                                                     │   permissions "<instr>"
                                                     │   → events::emit(
                                                     │     "checkpoint_auto_
                                                     │      decided", …) (D-07)
                                                     │   → re-enter the
                                                     │     monitor/advance loop
                                                     │     for the SAME stage
                                                     │
                                                     └─ not confirmed ─▶
                                                         handle_stage_failure()
                                                         (an ordinary failure
                                                         that merely happened
                                                         to occur in a phase
                                                         that HAS a
                                                         blocking-human task
                                                         somewhere)
```

### Recommended Project Structure

No new files. Every unit is a function added to an existing module:

```
crates/devflow-core/src/
├── verify.rs           # + phase_has_blocking_human_checkpoint() (D-01 scan)
├── agent_result.rs     # + session_id field on AgentResult; capture in
│                        #   parse_marker_lines / envelope parsing (D-04)
│                        # + Gate: field reader (D-01 confirm step)
├── agents/claude.rs     # + a --resume variant of exec_command (D-04/D-05)
├── state.rs             # + session_id: Option<String> field (D-04)
└── config.rs             # + yes_ship: bool field + yes_ship() resolver (D-12)

crates/devflow-cli/src/
├── prompt.rs (core, see above) — no CLI-side prompt change needed for D-14;
│    the fix is entirely inside devflow-core::prompt
├── pipeline_launch.rs    # advance(): D-01 dispatch insertion; resume():
│                          #   D-15's guarded clear
└── commands.rs           # start(): D-12's OR-combine of CLI flag + config
```

### Pattern 1: Static PLAN.md discovery (D-01) — reuse `verify.rs`'s exact loop

**What:** `verify::external_verify_commands` (`crates/devflow-core/src/verify.rs:34-67`)
already walks `.planning/phases/{NN}-*/` directories, collects every file
matching `{NN}-*-PLAN.md`, sorts them, and reads their contents. D-01 needs
the identical directory-and-file discovery loop, just a different per-file
predicate (a literal `gate="blocking-human"` substring search in the task
body, not YAML frontmatter parsing).

**When to use:** Before dispatching a non-success `advance()` outcome to the
existing `handle_stage_failure`/`handle_ship_failure` never-silent gate path.

**Example (current, confirmed-live code to mirror — not to modify):**
```rust
// Source: crates/devflow-core/src/verify.rs:34-67 (read live this session)
pub fn external_verify_commands(project_root: &Path, phase: u32) -> Vec<String> {
    let phases_dir = project_root.join(".planning/phases");
    let phase_prefix = format!("{phase:02}-");
    let plan_prefix = format!("{phase:02}-");
    let mut plans = Vec::<PathBuf>::new();

    let Ok(phase_entries) = std::fs::read_dir(phases_dir) else {
        return Vec::new();
    };
    for phase_entry in phase_entries.flatten() {
        if !phase_entry.file_name().to_string_lossy().starts_with(&phase_prefix) {
            continue;
        }
        let Ok(plan_entries) = std::fs::read_dir(phase_entry.path()) else { continue };
        plans.extend(plan_entries.flatten().filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name.starts_with(&plan_prefix) && name.ends_with("-PLAN.md")).then(|| entry.path())
        }));
    }
    plans.sort();
    plans.into_iter().filter_map(|path| std::fs::read_to_string(path).ok())
        // D-01's own predicate replaces this line:
        .filter_map(|contents| command_from_frontmatter(&contents))
        .collect()
}
```
The planner should specify a new sibling function (suggested name:
`verify::phase_has_blocking_human_checkpoint(project_root, phase) -> bool`)
that runs the same discovery loop and returns `true` on the first file whose
contents contain the literal substring `gate="blocking-human"`. A regex or
XML parse is unnecessary — task attributes are always double-quoted in GSD's
task XML, so a plain substring match is exact and matches this module's own
existing minimal-parser philosophy (see `verify.rs`'s doc comment: "This
intentionally small parser recognizes the scalar shape...").

### Pattern 2: `Gate:` field confirmation — the exact literal to match

**What:** `gsd-executor`'s `checkpoint_return_format` (confirmed live,
`$HOME/.claude/agents/gsd-executor.md:349-380`) emits, on every checkpoint,
a markdown block whose second line is:
```
**Gate:** [blocking | blocking-human] — copy the task's `gate` attribute verbatim so the orchestrator's carve-out sees it
```
i.e. the literal, confirmed output for a `blocking-human` checkpoint is a
line beginning `**Gate:** blocking-human`. This is what
`execute-phase.md`'s `checkpoint_handling` step (confirmed live,
`workflows/execute-phase.md:1053`) itself keys its own carve-out on: *"If the
returned `Gate:` is `blocking-human` ... never auto-approve or auto-select."*

**When to use:** After D-01's static scan finds a `gate="blocking-human"`
task declared *somewhere* in the stage's plan(s), confirm the specific
non-success exit actually IS that checkpoint (not an unrelated ordinary
failure) by searching the captured stdout (`.devflow/phase-NN-stdout`, the
same file `evaluate_layer1` already reads) for the literal substring
`**Gate:** blocking-human`.

**Important caveat surfaced by this research:** DevFlow launches the
**top-level orchestrator session** (`claude -p "/gsd-execute-phase N"`), not
the `gsd-executor` subagent directly — `gsd-executor` is spawned internally
via Claude Code's `Task` tool. What DevFlow's monitor captures in
`.devflow/phase-NN-stdout` is the **top-level session's own final printed
message**, which is `execute-phase.md`'s orchestrator relaying the
checkpoint (its `<checkpoint_handling>` "Present to user" formatting, or —
under headless `--dangerously-skip-permissions` with no human present — the
raw checkpoint content the orchestrator has nothing else to do with but
print before the process exits with no further input). The planner must
verify empirically (a live dogfood/probe run, not just static reading) that
`**Gate:**` (or the orchestrator's own re-rendering of the `[Type]`/gate
info) actually survives into the captured top-level stdout verbatim, since
this is a chain of two indirections (`gsd-executor` → `execute-phase.md`
orchestrator → DevFlow's captured stdout) that this research could confirm
in source but not in a live run this session. Flag as an assumption (see
Assumptions Log A1) requiring a `checkpoint:human-verify`-class live probe
early in the phase's plan, before building the confirmation-reader logic
around an unverified string shape.

### Pattern 3: `session_id` capture — exact envelope shape (verified live)

**What:** `session_id` already appears in every Claude JSON-envelope test
fixture in `agent_result.rs`, but is never read into `AgentResult`. Ground
truth fixture (confirmed at the exact line CONTEXT.md cited,
`agent_result.rs:1362`, re-read live this session — unchanged):
```rust
// Source: crates/devflow-core/src/agent_result.rs:1362 (read live this session)
r#"{"type":"result","is_error":true,"num_turns":3,"result":"oops\nDEVFLOW_RESULT: {\"status\":\"success\"}","session_id":"abc"}"#
```
Six other fixtures in the same file (lines 1272, 1288, 1351, 1373, 1379,
1477) all carry `"session_id":"abc"` or `"session_id":"x"` at the same
top level, alongside `type`, `subtype`, `is_error`, `num_turns`, `result`.
This is also independently confirmed by Claude Code's own docs (WebFetch,
`code.claude.com/docs/en/sessions`, "Access conversations from scripts"):
`--output-format json` returns "the result, session ID, usage, and cost."

**When to use:** Add `session_id: Option<String>` to `AgentResult` (mirrors
the existing `#[serde(default)]` optional-field pattern already used for
`verdict`/`decided_by_layer`). Extract it in `extract_json_result_text` or a
sibling helper, and thread it into `State.session_id` at the same point
`advance()` already reads `result.status`/`result.reason` (`pipeline_launch.rs`,
`advance()` function, right after `evaluate_agent_result` returns).

### Pattern 4: `--resume` relaunch — confirmed exact CLI syntax and a load-bearing gotcha

**What:** Confirmed via WebFetch against `code.claude.com/docs/en/sessions`
(current official docs, fetched live this session) — the documented,
intended scripting pattern is exactly D-04's claim:
```bash
claude -p --resume <session-id> --output-format json "<prompt>"
```
Docs' own words: *"Sessions created with `claude -p` ... do not appear in
the session picker, but you can still resume one by passing its session ID
to `claude --resume <session-id>`. **Run this from the directory the session
was started in**: session ID lookup is scoped to the current project
directory and its git worktrees."* This directly confirms D-04's claim that
`spawn_monitor` always launching from `state.worktree_path` (confirmed live,
`monitor.rs:96-100`) satisfies `--resume`'s directory requirement — as long
as `state.worktree_path` does not change between the checkpoint's original
launch and the relaunch, which it cannot within a single stage/phase run.

**When to use:** D-04's relaunch path, added to `agents/claude.rs` as a
Claude-only method (not on the shared `AgentAdapter` trait, per D-05).

### Anti-Patterns to Avoid

- **Assuming `--resume` restores permission mode.** See Pitfall 1 — it does
  not, and this is the single highest-consequence gap this research found.
- **Treating the `Gate:` field as guaranteed to survive unmodified into
  captured stdout.** It is emitted by a subagent, relayed by an orchestrator
  session, and only THEN captured by DevFlow. Verify empirically (Pattern 2).
- **Reimplementing PLAN.md discovery from scratch.** `verify.rs` already has
  the exact walk; a second, slightly-different implementation is exactly the
  kind of duplicated "what does the agent/plan mean" logic D-02 explicitly
  rejected for pattern-matching failure text — the same principle applies to
  duplicating file-discovery logic.
- **Reading `state.yes_ship` inside `run_gate_with_timeout`.** The existing
  code has an explicit, tested invariant against this (see
  `pipeline_gate.rs:287-296`'s doc comment and the regression test
  `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`) — D-12
  must extend `commands::start`'s resolution of `state.yes_ship`, never make
  `run_gate_with_timeout` itself consult config.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Finding a phase's PLAN.md file(s) | A new directory-walk/glob implementation | The existing loop in `verify::external_verify_commands` (`verify.rs:34-67`), factored into a shared helper or copied verbatim with a different predicate | Two independent discovery implementations for the same `.planning/phases/{NN}-*/{NN}-*-PLAN.md` convention will drift the moment one is updated and the other isn't. |
| Recording "what the agent decided unattended" | A new file format under `.devflow/` | `events::emit()` into the existing `.devflow/events.jsonl` (schema v1, append-only, already the read side of the notify hook) | `events.jsonl` is already the single, tailable audit log every other gate/transition event uses (`gate_fired`, `gate_resolved`, `notify_fired`, `advance_evaluated`, `capture_archived`). A parallel file fragments the audit trail the very next `devflow doctor`/`devflow status`/Hermes reconciliation pass has to reconcile against. |
| Config precedence for a new toggle (`yes_ship`) | A bespoke env-var-then-file resolver | The existing `external_verify_enabled(project_root)` pattern in `config.rs:151-163` (env var > `devflow.toml` > built-in default via `DevflowConfig::default()`) | This exact resolver shape is already implemented, tested, and documented for `capture_retention`/`review_angles`/`external_verify_enabled`. A fourth, differently-shaped resolver for `yes_ship` would be the odd one out for no reason. |

**Key insight:** this phase's entire surface area is additive extensions of
patterns DevFlow already implements four times over (state field,
config field, event kind, file-discovery loop). The risk is not "what
library do I need" — it is "did I actually reuse the existing shape, or did
I quietly reinvent a fifth slightly-different version of it."

## Common Pitfalls

### Pitfall 1: `--resume` does not restore `--dangerously-skip-permissions`

**What goes wrong:** The relaunched session silently reintroduces the exact
headless-hang hazard this whole phase exists to close — a permission prompt
with no human present to answer it, on a `--resume`d session launched
specifically to run *unattended*.

**Why it happens:** Confirmed via WebFetch against the official Claude Code
docs (`code.claude.com/docs/en/sessions`, "What a resumed session restores"):
*"Permission mode: the mode the session was in. `plan` and
`bypassPermissions` are never restored; bypassing permissions must be
enabled again at launch, with one of its launch flags..."* DevFlow's
`ClaudeAgent::exec_command` (`agents/claude.rs:15-31`) always includes
`--dangerously-skip-permissions` on the *original* launch — but this is
easy to forget to re-add on the new `--resume` relaunch code path since it
"feels like" a continuation of an already-configured session.

**How to avoid:** The D-04 relaunch command must explicitly include
`--dangerously-skip-permissions` (and `--output-format json`, also not
restored per the same docs page) on every `--resume` invocation, not just
the original. Suggested exact command:
```bash
claude -p --resume "$session_id" --output-format json --dangerously-skip-permissions "<synthesized instruction>"
```

**Warning signs:** A relaunched checkpoint session that never produces a
`DEVFLOW_RESULT` marker and never exits — the monitor's `wait $apid` blocks
forever exactly as it would for any other silently-hung agent, but the root
cause (a permission prompt with no terminal) is invisible in captured stdout
because the prompt itself may not even be flushed to stdout before blocking
on stdin. Bounded-timeout probing (not blind trust) is the way to catch this
in the plan's verification loop.

### Pitfall 2: The `Gate:` field is two indirections away from DevFlow's captured stdout

**What goes wrong:** D-01's confirmation step (searching captured stdout for
`**Gate:** blocking-human`) is built against a string this research
confirmed exists in `gsd-executor`'s own structured return
(`gsd-executor.md:356`) and in `execute-phase.md`'s own carve-out matching
logic (`execute-phase.md:1053`) — but DevFlow does not capture
`gsd-executor`'s output directly. It captures the **top-level orchestrator
session's** final stdout, and that orchestrator (running
`execute-phase.md`'s `checkpoint_handling` step) is the one that receives
the subagent's checkpoint return and decides what to print. Under
`--dangerously-skip-permissions` with genuinely no human present (no TTY,
no `AskUserQuestion` answer forthcoming), the exact final text the
orchestrator prints before the process exits with nothing left to do is not
verified in this research session — only its two endpoints are (the
subagent's emission, and the orchestrator's own reference to the `Gate:`
field it expects to see).

**Why it happens:** `checkpoint_handling`'s "Standard flow" step 4
("Present to user") assumes an interactive terminal exists to present to.
Headless `claude -p` has no such terminal.

**How to avoid:** Treat the exact captured-stdout string shape as an
assumption requiring live confirmation (a scoped dogfood/probe run against
a phase containing a genuine `gate="blocking-human"` task, executed via
`devflow start`) before or alongside building the D-01 confirmation-reader
logic, not purely from static reading of the two markdown source files.
Flagged in Assumptions Log (A1).

**Warning signs:** D-01's confirmation logic silently falls through to the
"not confirmed" branch (ordinary `handle_stage_failure`) even when a real
`blocking-human` checkpoint fired — the worst-case failure mode is quiet,
not loud, since the fallback path is itself a legitimate, expected outcome
(an unrelated stage failure in a phase that happens to have a
blocking-human task elsewhere).

### Pitfall 3: `--yes-ship`'s existing "override" doc comment will read as stale/wrong after D-12

**What goes wrong:** `commands.rs:125-128`'s comment currently reads *"The
only assignment in the crate that ever sets `yes_ship` to a non-default
value"* — true today, false the moment D-12 lands (a config-file value can
also set it, via a value combined at this same call site). Leaving the
comment unchanged after the code changes would be a load-bearing doc-vs-code
drift precisely at the site three other regression tests reference by name
(`finalization_retry_gate_never_auto_approves_even_with_yes_ship_set`,
`run_preflight_major_bump_gate_not_auto_approved_by_yes_ship`) to explain
*why* those tests exist.

**How to avoid:** Update the comment at the same time as the code (this is
explicitly the kind of "your own changes" cleanup CLAUDE.md's Surgical
Changes rule calls for — the comment directly describes the line being
changed).

### Pitfall 4: `Stage::Define`'s idempotency branch removal must not disturb the Plan-stage sibling

**What goes wrong:** `idempotent_stage_prompt` (`prompt.rs:142-166`) is
shared between `Stage::Define` and `Stage::Plan` (see
`stage_prompt_with_project`'s `matches!(stage, Stage::Define | Stage::Plan)`
dispatch, `prompt.rs:197-199`). D-14's fix must change ONLY the
`Stage::Define` "does not exist" arm's behavior (skip invoking
`/gsd-discuss-phase` — proceed without CONTEXT.md), while `Stage::Plan`'s
identical-shaped branch (invoking `/gsd-plan-phase`, which is NOT interactive
in the same blocking way) must be left untouched. The existing test
`define_and_plan_prompts_are_idempotent` (`prompt.rs:365-388`) currently
asserts BOTH stages share the "run the GSD command when missing" behavior —
this test will need to split into two once D-14 lands, one asserting Define's
new no-op-without-artifact behavior and one preserving Plan's existing
behavior. Get this wrong and either Plan stops running when it should, or
Define keeps invoking the interactive interview D-14 exists to prevent.

## Code Examples

Verified patterns from live source, re-read this session (all line numbers
confirmed current, no drift from CONTEXT.md's citations):

### Existing `State` backward-compat field pattern (model for `session_id`)
```rust
// Source: crates/devflow-core/src/state.rs:71-79 (read live this session)
/// PID of the detached monitor process that owns the agent for the
/// current stage, recorded by `launch_stage` at spawn time. `None` means
/// no monitor has been spawned for this state yet, OR the state was
/// written by a binary predating this field — in both cases the
/// liveness probe reports Unknown, never Stuck.
#[serde(default)]
pub monitor_pid: Option<u32>,
```
`session_id: Option<String>` should follow this exact shape:
`#[serde(default)]`, `Option<...>`, doc comment stating explicitly what
`None` means for both "never captured yet" and "written by a pre-28 binary."

### Existing config resolver pattern (model for `yes_ship`)
```rust
// Source: crates/devflow-core/src/config.rs:149-163 (read live this session)
/// Resolve external verification with `DEVFLOW_EXTERNAL_VERIFY_ENABLED`
/// taking precedence over `devflow.toml` and the built-in default.
pub fn external_verify_enabled(project_root: &Path) -> bool {
    if let Some(value) = env_value("DEVFLOW_EXTERNAL_VERIFY_ENABLED") {
        match value.parse() {
            Ok(enabled) => return enabled,
            Err(error) => tracing::warn!(
                value, %error,
                "invalid DEVFLOW_EXTERNAL_VERIFY_ENABLED; using devflow.toml or default"
            ),
        }
    }
    load_config(project_root).external_verify_enabled
}
```
A `pub fn yes_ship(project_root: &Path) -> bool` resolver following this
exact shape (with a `DEVFLOW_YES_SHIP` env override, `devflow.toml`'s new
`yes_ship = true` key, default `false`) is the discretion-resolved answer to
"exact config key name/shape... and where in `devflow-core::config` it's
parsed." Combine at the `commands::start` call site (`commands.rs:129`):
```rust
state.yes_ship = yes_ship /* CLI flag */ || devflow_core::config::yes_ship(project_root);
```
This is logical OR because the CLI flag has no "false" form (a bare boolean
`#[arg(long)]` flag; there is no `--no-yes-ship`) — "the CLI flag still
overrides" (D-12) is satisfied trivially: passing `--yes-ship` always wins
regardless of config, and omitting it falls through to the config value.

### The `Gate:` line, exact confirmed literal (D-01's confirmation target)
```
// Source: $HOME/.claude/agents/gsd-executor.md:356 (read live this session)
**Gate:** [blocking | blocking-human] — copy the task's `gate` attribute verbatim so the orchestrator's carve-out sees it
```
For a real `blocking-human` checkpoint this renders literally as a line
beginning `**Gate:** blocking-human`.

### Existing `events::emit` call shape (model for D-07's audit record)
```rust
// Source: crates/devflow-cli/src/pipeline_gate.rs:315-324 (read live this session)
events::emit(
    project_root,
    state.phase,
    "gate_fired",
    serde_json::json!({
        "stage": stage.to_string(),
        "unexpected": unexpected,
        "context": context,
    }),
);
```
D-07's new event kind (suggested name: `"checkpoint_auto_decided"`) should
follow this exact call shape — same `events::emit(project_root, phase,
"<event-name>", serde_json::json!({...}))` signature, emitted at the point
DevFlow relaunches via `--resume` (Pattern 4), carrying at minimum
`stage`, `session_id`, and the synthesized instruction text (truncated via
the existing `render_gate_context`/`truncate_reason`, `pipeline_outcomes.rs:318-344`,
the same 300-char construction-time cap already applied to every other
agent-controlled string reaching a gate/notify/event).

### `--resume` command construction (D-04, confirmed exact syntax)
```rust
// New Claude-only relaunch entry point — sketch, not verbatim source
// (this function does not exist yet; model on ClaudeAgent::exec_command,
// crates/devflow-core/src/agents/claude.rs:15-31, read live this session)
pub fn resume_command(session_id: &str, instruction: &str) -> (&'static str, Vec<String>) {
    (
        "claude",
        vec![
            "-p".into(),
            instruction.to_string(),
            "--resume".into(),
            session_id.to_string(),
            "--output-format".into(),
            "json".into(),
            // Pitfall 1: NOT restored by --resume — must be re-passed.
            "--dangerously-skip-permissions".into(),
        ],
    )
}
```

## Discretion Resolutions

Concrete recommendations for the 5 items CONTEXT.md left to the
researcher/planner:

1. **Synthesized instruction wording (D-04).** Keep it short, factual, and
   explicit that this is an unattended, policy-driven decision (not
   pretending to be a human). Suggested shape: *"No human operator is
   available to answer this checkpoint. Per DevFlow's unconditional
   auto-decide policy (see project D-03), use your own best judgment to
   resolve it and continue. Record your reasoning in your final message."*
   Keep it stable/deterministic (no timestamp or random content) so the
   audit record (D-07) can quote it verbatim without churn.

2. **Audit record shape/location (D-07).** Reuse `events::emit()` into the
   existing `.devflow/events.jsonl` — new event kind
   `"checkpoint_auto_decided"` carrying `stage`, `session_id`,
   `synthesized_instruction` (or a stable hash/truncation of it), and a
   `truncate_reason`-capped excerpt of the agent's resumed response. This
   is a genuine extension of Phase 23's D-06 gate-ledger pattern — the
   ledger already lives in `events.jsonl` (`gate_fired`/`gate_resolved`
   pairs); this is a third, sibling event kind in the same log, not a new
   file.

3. **`yes_ship` config key/shape (D-12).** Add `pub yes_ship: bool` to
   `DevflowConfig` (`config.rs:52-59`), default `false` in its `Default`
   impl (`config.rs:61-69`) — the struct's existing `#[serde(default)]`
   struct-level attribute already covers an absent key in `devflow.toml`.
   Add a `pub fn yes_ship(project_root: &Path) -> bool` resolver mirroring
   `external_verify_enabled` exactly (env var `DEVFLOW_YES_SHIP` overrides
   `devflow.toml`'s `yes_ship = true` overrides the `false` default).

4. **Where the static PLAN.md scan lives (D-01).** `devflow-core::verify` —
   a new `pub fn phase_has_blocking_human_checkpoint(project_root: &Path,
   phase: u32) -> bool`, sibling to `external_verify_commands` in the same
   file, reusing its exact directory-walk shape (Pattern 1). Call it from
   `pipeline_launch::advance`'s `Action::GateReview` dispatch arm, before
   today's unconditional fall-through to `handle_stage_failure`/
   `handle_ship_failure`.

5. **`session_id` persistence shape on `State` (D-04).** `pub session_id:
   Option<String>` with `#[serde(default)]`, added next to `monitor_pid` in
   `state.rs` (same struct region, same doc-comment convention: explain what
   `None` means both for "never captured" and "state written by a pre-28
   binary"). Follows the exact backward-compat test pattern already present
   four times in `state.rs` (`monitor_pid_absent_from_json_defaults_to_none`,
   `yes_ship_absent_from_json_defaults_to_false`,
   `stop_fields_absent_from_json_default`,
   `infra_failures_absent_from_json_defaults_to_zero`) — the plan should
   include an equivalent `session_id_absent_from_json_defaults_to_none` test.

## Runtime State Inventory

Not applicable — this is not a rename/refactor/migration phase. No stored
data, live-service config, OS-registered state, secrets, or build artifacts
carry a name being changed. Skip entirely per the trigger condition.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| `claude` CLI (headless `-p`/`--resume`) | D-04's relaunch path; already required by every existing Claude-agent stage | Already a hard existing dependency of `AgentKind::Claude` — nothing new to probe. `ensure_agent_binary("claude")` already gates every launch. | — (this phase does not pin a version; Claude Code's `--resume` behavior documented above is current as of this research date) | None needed — D-05 scopes this phase to Claude only; if `claude` is unavailable, the existing pre-launch `ensure_agent_binary` check already fails the run before D-04's code path is ever reached. |
| `cargo`/Rust toolchain | Building/testing the four units | Already the project's sole build system; no new probe needed. | — | — |

No new external dependency is introduced, so the broader audit (databases,
network services, other language runtimes) does not apply.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in Rust test harness), workspace-wide |
| Config file | `scripts/check.sh` — the single canonical "is this green?" definition (also run by CI and the pre-push hook) |
| Quick run command | `cargo test -p devflow-core <module>::tests::` (targeted, e.g. `cargo test -p devflow-core state::tests::`) |
| Full suite command | `scripts/check.sh test` (== `cargo test --workspace`), or `scripts/check.sh all` for fmt+clippy+test |

### Phase Requirements → Test Map

| Unit | Behavior | Test Type | Automated Command | File Exists? |
|------|----------|-----------|---------------------|--------------|
| 28a (D-01) | Static PLAN.md scan detects a `gate="blocking-human"` task | unit | `cargo test -p devflow-core verify::tests::phase_has_blocking_human_checkpoint` (new test, name illustrative) | ❌ new test in `verify.rs` |
| 28a (D-01) | Scan returns `false` when no task carries the attribute (today's path unchanged) | unit | `cargo test -p devflow-core verify::tests::` | ❌ new test alongside above |
| 28a (D-04) | `session_id` round-trips through `AgentResult`/`State` serde, absent-defaults-to-None | unit | `cargo test -p devflow-core state::tests::session_id` / `agent_result::tests::` | ❌ new tests, mirroring 4 existing sibling tests |
| 28a (D-04/D-05) | `--resume` command construction includes `--dangerously-skip-permissions` and `--output-format json` (Pitfall 1 regression guard) | unit | `cargo test -p devflow-core agents::claude::tests::resume_command_includes_permission_bypass` (new, illustrative name) | ❌ new test |
| 28b (D-07) | Auto-decide fires exactly one `checkpoint_auto_decided` event, never silent | integration | `cargo test -p devflow-cli pipeline_launch::tests::` or a new `pipeline_outcomes.rs` test mirroring `handle_ship_outcome_with_yes_ship_auto_approves_exactly_once_with_attribution`'s shape | ❌ new test |
| 28c (D-14) | Define with missing CONTEXT.md no-ops without invoking `/gsd-discuss-phase` | unit | `cargo test -p devflow-core prompt::tests::` — extend/split `define_and_plan_prompts_are_idempotent` (`prompt.rs:365-388`) | ✅ existing test needs splitting, not creating from scratch |
| 28c (D-14) | Plan's existing idempotent-artifact behavior is unaffected | unit | Same test file, the split-off Plan half | ✅ existing coverage, preserve |
| 28d (D-15) | `resume` does NOT clear an unfired `--until` cap (`state.stopped == false`) | unit | `cargo test -p devflow-cli pipeline_launch::tests::` — new sibling to `resume_clears_stop_marker_and_advances_past_stop_point` (`pipeline_launch.rs:456-526`) | ❌ new test, existing sibling to model on (proves the OPPOSITE case: `stopped: false` on entry must stay untouched) |
| D-12 | `yes_ship` config file key sets `state.yes_ship` when CLI flag omitted | integration | `cargo test -p devflow-cli pipeline_outcomes::tests::config_file_with_yes_ship_key_loads_but_never_sets_the_flag` — **this exact test's assertion must flip** from `!state.yes_ship` to a new positive-case test | ✅ existing test, assertion inversion required |
| D-12 | CLI flag still wins/ORs correctly over config | integration | New test alongside the flipped one above | ❌ new test |

### Sampling Rate

- **Per task commit:** `cargo test -p devflow-core <touched module>::tests::`
  or `cargo test -p devflow-cli <touched module>::tests::` (targeted to the
  file just changed).
- **Per wave merge:** `scripts/check.sh test` (full `cargo test --workspace`).
- **Phase gate:** `scripts/check.sh all` (fmt + clippy + test) green before
  `/gsd-verify-work`, matching this project's existing standard (`scripts/check.sh`
  is the single definition of green used by CI, the pre-push hook, and local
  dev — confirmed live, `scripts/check.sh:1-8`).

### Wave 0 Gaps

None — existing test infrastructure (`cargo test --workspace`, the
`ENV_MUTEX`-serialized PATH-stubbing pattern already used throughout
`pipeline_launch.rs`/`preflight.rs`/`gates.rs` for env-dependent tests, and
the `tempfile`-backed fixture-repo pattern already used throughout) covers
every phase requirement. No new test framework, fixture harness, or shared
conftest-equivalent is needed — every new test in the table above is a
straightforward addition to an already-present `#[cfg(test)] mod tests`
block in the touched file, following patterns already exercised dozens of
times in the same files (see the four `_absent_from_json_default*` tests in
`state.rs` alone).

## Security Domain

`security_enforcement` is not set to `false` in `.planning/config.json` —
included per default-enabled policy. This phase's core mechanism (D-03) is
itself a **deliberate, informed override of a GSD-core-wide security
invariant** (`checkpoints.md` rule 6), so this section is unusually
load-bearing — it documents what that override actually exposes, not merely
generic ASVS boilerplate.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|-------------------|
| V2 Authentication | No | No new auth surface — `claude -p --resume` reuses the operator's own already-authenticated local Claude Code session/credentials; no new credential material is introduced. |
| V3 Session Management | Yes | `session_id` is treated as an opaque, DevFlow-internal capability token scoped to one phase's `State` — never logged in full to `events.jsonl` (only truncated/hashed instruction text is recorded per Discretion Resolution 2), never exposed via `devflow status`'s human-facing output beyond what's already shown for `monitor_pid`. |
| V4 Access Control | Yes (indirectly) | D-03's unconditional auto-decide is itself the access-control decision under review here — see STRIDE table below. The mitigation is D-07's mandatory audit trail plus this phase's explicit non-goal of expanding WHICH gate types this applies to (only `gate="blocking-human"` checkpoints already present in a plan the operator approved at Plan-stage). |
| V5 Input Validation | Yes | The `Gate:` field confirmation (D-01) must not treat an agent-controlled string as a trusted signal for anything beyond "route to the auto-decide path vs. the existing generic error-gate path" — both destinations are already-safe, already-audited code paths (`handle_stage_failure` / the new D-04 relaunch, itself gated by D-07's audit). An agent that fabricates a `**Gate:** blocking-human` line in its own output to trigger the auto-decide path gains nothing it didn't already have (it can already choose to exit non-zero and reach `handle_stage_failure` today); this is analyzed explicitly below (Threat T-28-01). |
| V6 Cryptography | No | No cryptographic material is introduced by this phase. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| An agent (compromised, confused, or adversarially prompted) fabricates a `**Gate:** blocking-human` line to route itself into the D-04 auto-decide relaunch path instead of the ordinary never-silent gate. | Spoofing / Elevation of Privilege | **Accept, with a documented rationale (T-28-01):** D-01's static PLAN.md scan is the PRIMARY gate — the auto-decide path is only reachable when the *stage's own plan, authored and approved during Plan-stage*, declares a `gate="blocking-human"` task. An agent cannot fabricate its way into this path for a phase whose plans never declared one, because the static scan runs first and is read from `.planning/phases/` (agent-writable during Code, but the same trust boundary already accepted by `verify::external_verify_commands`'s existing Layer 0 mechanism — see its own doc comment: "Because those files are agent-writable, execution additionally requires ... authorization" — D-01 does not introduce a NEW instance of this trust boundary, it reuses the existing one). Within a phase that DOES declare `gate="blocking-human"` somewhere, the worst a fabricated `Gate:` line achieves is routing an *ordinary* failure into the auto-decide relaunch instead of the generic gate — and D-07 makes that substitution unconditionally auditable, closing the D-09/`checkpoints.md` rule-6 concern ("never silently authorized") the same way D-07 was designed to. |
| The `--resume` relaunch omits `--dangerously-skip-permissions`, and the resumed session blocks indefinitely on a permission prompt no human can answer. | Denial of Service | **Mitigate:** Pitfall 1's fix (always re-pass `--dangerously-skip-permissions` and `--output-format json` on the resume command) plus a bounded-timeout regression test asserting the relaunch command's exact argv shape (see Validation Architecture table, "resume_command_includes_permission_bypass"). |
| D-03's unconditional auto-decide is invoked for an irreversible action the plan's own checkpoint text warns is high-stakes (the exact Phase 26 near-miss scenario: a mistagged `gate="blocking"` vs. `"blocking-human"` task authorizing `cargo publish`). | Elevation of Privilege | **Accept, per explicit, twice-reaffirmed operator decision (D-03) — not a defect this phase introduces or can close.** This phase does not change WHICH tasks are tagged `blocking-human` (that remains a planner/operator-authored classification at Plan-stage, unchanged); it changes what happens AFTER such a task blocks with no human present. D-07's audit trail is the accepted mitigation, matching the operator's own explicit risk acceptance recorded in CONTEXT.md D-03's "Reversibility: costly" note. The planner should not attempt to re-litigate or "improve" this via, e.g., a package-legitimacy special-case — that was the exact umbrella-flag proposal D-06 already rejected. |
| `session_id` persisted on disk (`.devflow/state-NN.json`) grants whoever can read that file the ability to `claude --resume` into a live/exited session and issue arbitrary further instructions inside it. | Elevation of Privilege / Information Disclosure | **Accept, scoped to existing trust boundary:** `.devflow/state-NN.json` already contains `worktree_path`, `project_root`, and other operationally-sensitive fields with the same local-filesystem trust boundary as every other DevFlow state file — anyone with filesystem access to `.devflow/` already has equivalent-or-greater capability (they can read `monitor_pid` and signal the process, read stdout captures with full agent output, etc.). `session_id` does not create a NEW trust boundary; it is protected exactly as well as everything else already stored there. No new mitigation required beyond existing filesystem permissions on `.devflow/`. |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|----------------|
| A1 | The literal string `**Gate:** blocking-human` (or an equivalent, reliably-parseable rendering of the executor's `Gate:` field) actually survives, unmodified, into DevFlow's captured top-level stdout when a `gate="blocking-human"` checkpoint fires under `claude -p --dangerously-skip-permissions` with no human present. Confirmed in *source* (both `gsd-executor.md` and `execute-phase.md` reference this exact field), but NOT confirmed against a *live headless run* this session. | Architecture Patterns, Pattern 2; Common Pitfalls, Pitfall 2 | If the string shape differs (e.g. the orchestrator reformats it, truncates it, or a compaction/summary step drops it before the process prints its final message and exits), D-01's confirmation step silently falls through to the "not confirmed" branch and every genuine `blocking-human` checkpoint routes to the ordinary never-silent gate instead of D-03's auto-decide path — which is actually the SAFE failure direction (falls back to today's behavior, does not silently over-trigger auto-decide), but means the phase's primary deliverable (28a) does not actually work end-to-end until this is empirically confirmed and possibly adjusted. |
| A2 | `claude -p --resume <id> --output-format json` on a session that was originally launched with `--dangerously-skip-permissions` requires that flag to be re-passed on the resume invocation (per official docs' general statement that permission mode "must be enabled again at launch"), rather than the flag being scoped to first-launch-only in some way not documented. Confirmed via official docs (WebFetch, this session) but not via a live empirical test against this specific flag combination. | Common Pitfalls, Pitfall 1; Code Examples | If wrong (i.e. if `--dangerously-skip-permissions` were somehow inherited or irrelevant to a resumed session), the planner's regression test asserting its presence is harmless-but-unnecessary — low risk either direction, hence classified LOW/MEDIUM confidence rather than blocking. |

## Open Questions

1. **Does the `execute-phase.md` orchestrator's own final printed message, under fully headless `claude -p` with `checkpoint_handling`'s "Standard flow" step 4 having no terminal to present to, actually terminate the session cleanly (exit 0/printing something parseable) or does it hang waiting for input that will never come?**
   - What we know: `checkpoint_handling`'s standard flow assumes an interactive terminal ("Present to user," "User responds"). Auto-mode's carve-out explicitly excludes `blocking-human` from auto-approval, so a `blocking-human` checkpoint under `AUTO_MODE=true` still falls through to "Standard flow," not the auto-approve branches.
   - What's unclear: whether `claude -p`'s underlying agent loop, when it reaches a step that would normally ask for terminal input via `AskUserQuestion` or an equivalent, resolves this by exiting with a printed message (which DevFlow could then read) or by hanging (which DevFlow's monitor would eventually see as a dead process with no exit code recorded — Layer 3 territory in `agent_result.rs`).
   - Recommendation: this is the load-bearing empirical question underlying A1. The plan should include an early, scoped probe task (a synthetic phase with a trivial `gate="blocking-human"` checkpoint task, run via `devflow start` end-to-end) before committing to the exact string-matching logic for D-01's confirmation step — CONTEXT.md's own framing already anticipates this ("Verified this caveat does not transfer to DevFlow's shape... cited here so a future reader doesn't re-raise it without that context" refers to a DIFFERENT caveat about parallel tool calls, not this one).

2. **Is a truncated/hashed `session_id` sufficient for the D-07 audit record, or should the full value be recorded?**
   - What we know: `.devflow/events.jsonl` is already the audit trail for gate resolution, and other event kinds record full field values (e.g. `gate_resolved`'s `responded_by`).
   - What's unclear: whether recording the full `session_id` in an append-only, potentially-committed-to-git-adjacent log (`.devflow/` itself is typically gitignored per this project's own conventions, based on `19a`'s hygiene fixes referenced in STATE.md — but worth the planner confirming `.devflow/` is still gitignored before deciding) poses any practical risk beyond the already-accepted filesystem trust boundary (see Security Domain's third threat row).
   - Recommendation: record the full `session_id` — it provides the most diagnostic value for a human reconstructing what happened after the fact (D-07's entire purpose), and per Security Domain's analysis this introduces no new trust boundary beyond what `.devflow/state-NN.json` already exposes.

## Sources

### Primary (HIGH confidence — read live, this session, against the actual repository/reference files)
- `crates/devflow-core/src/gates.rs` — full file read
- `crates/devflow-core/src/agent_result.rs` — full file read (both pages)
- `crates/devflow-core/src/agents/claude.rs` — full file read
- `crates/devflow-core/src/agents/mod.rs` — full file read
- `crates/devflow-core/src/state.rs` — full file read
- `crates/devflow-core/src/prompt.rs` — full file read
- `crates/devflow-core/src/stage.rs` — full file read
- `crates/devflow-core/src/monitor.rs` — full file read
- `crates/devflow-core/src/config.rs` — read (lines 1-170)
- `crates/devflow-core/src/verify.rs` — read (lines 1-100)
- `crates/devflow-core/src/events.rs` — read (lines 1-80)
- `crates/devflow-cli/src/pipeline_launch.rs` — full file read
- `crates/devflow-cli/src/pipeline_outcomes.rs` — read (lines 230-650, 1600-1770)
- `crates/devflow-cli/src/pipeline_gate.rs` — read (lines 1-90, 261-385)
- `crates/devflow-cli/src/commands.rs` — read (lines 90-140)
- `crates/devflow-cli/src/main.rs` — read (lines 60-100)
- `$HOME/.claude/gsd-core/references/checkpoints.md` — grepped for rule 6, `blocking-human`
- `$HOME/.claude/gsd-core/workflows/execute-phase.md` — read (lines 1000-1100, `checkpoint_handling` step)
- `$HOME/.claude/agents/gsd-executor.md` — read (lines 330-390, `checkpoint_return_format`)
- `.planning/phases/28-close-the-checkpoint-answer-return-path/28-CONTEXT.md` — full file read
- `.planning/STATE.md` — read (lines 1-472)
- `.planning/config.json` — read
- `Cargo.toml` / `crates/devflow-core/Cargo.toml` — dependency graph check

### Secondary (MEDIUM confidence — official docs, WebFetch/WebSearch this session)
- [Manage sessions — Claude Code Docs](https://code.claude.com/docs/en/sessions) — confirmed exact `claude -p --resume <id> --output-format json` syntax, confirmed directory-scoping requirement, confirmed permission-mode is NOT restored on resume (Pitfall 1's source), confirmed `--output-format json` returns "the result, session ID, usage, and cost."

### Tertiary (LOW confidence — not used for any load-bearing claim in this document)
- WebSearch result snippets (SFEIR Institute, repovive.com, etc.) — used only to locate the authoritative docs page above; no claim in this document rests on these secondary aggregator sites alone.

## Metadata

**Confidence breakdown:**
- Standard stack: N/A — no new dependency.
- Architecture: HIGH — every canonical file re-read live this session; zero
  line-number drift found versus CONTEXT.md's citations.
- Pitfalls: HIGH for Pitfall 1 (confirmed against official docs, not merely
  training knowledge) and Pitfalls 3/4 (confirmed against live source);
  MEDIUM for Pitfall 2 (confirmed the two source-level references exist, but
  the end-to-end captured-stdout shape is unverified empirically — see A1).
- Security: MEDIUM — the threat analysis is HIGH confidence (grounded in
  live-read source and the operator's own already-recorded reasoning in
  CONTEXT.md D-03/D-09), but the accept-vs-mitigate dispositions reuse this
  project's own existing risk-acceptance precedent rather than introducing
  new, independently-validated mitigations — appropriate given D-03 is
  itself an explicit, twice-reaffirmed operator risk acceptance this
  research does not have standing to override.

**Research date:** 2026-07-30
**Valid until:** ~14 days (this phase touches fast-moving Claude Code CLI
behavior — `--resume`'s exact restore/non-restore semantics are the kind of
detail that changes between Claude Code releases; re-verify against
`code.claude.com/docs/en/sessions` if planning is delayed more than two
weeks from this research date). DevFlow's own source-level findings (line
numbers, function shapes) are stable until the next commit touches these
files — check `git log --oneline -- <file>` for the cited files if picking
this research back up after a gap.
