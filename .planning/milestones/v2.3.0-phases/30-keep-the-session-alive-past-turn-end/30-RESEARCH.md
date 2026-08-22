# Phase 30: Keep the Session Alive Past Turn End - Research

**Researched:** 2026-08-02
**Domain:** Rust CLI process orchestration — parsing Claude Code's `stream-json` event
protocol; de-risking a monitor-process rewrite before it is planned
**Confidence:** HIGH for source-code and archived-evidence claims (all file:line and
JSONL-line cited, independently re-read this session); LOW/ASSUMED for CLI-internal
behavior (`task-notification` delivery semantics) — the review that produced this
phase's binding constraints explicitly flags that behavior as undocumented and
unpinned upstream (finding M2).

No `CONTEXT.md` exists for this phase (operator planned directly from the ROADMAP
entry). The ROADMAP.md phase section (`### Phase 30: Keep the Session Alive Past Turn
End (999.64)`, lines 2253+) is the authoritative source of locked decisions and scope
fences and is reproduced/normalized below in place of a `CONTEXT.md`.

<user_constraints>
## User Constraints (from ROADMAP.md Phase 30 entry — no CONTEXT.md exists)

### Locked Decisions

- **Scope is 999.64 alone.** 999.65 and 999.66 are explicitly out of scope even
  though found the same day — they are sequenced after this phase because they
  cannot be observed until 999.64 lands. Do not fold them in.
- **No launch-path change in this phase (planner directive, verbatim from
  ROADMAP).** The planner MUST NOT pull Phase 31's monitor rewrite or adapter
  flag/argv switch forward into this phase. This phase is the parser
  (30b) and the de-risking experiments (30c, 30d) only.
- **Six binding review constraints (Claude Fable 5 adversarial review,
  2026-08-01) are locked design inputs**, reproduced verbatim below. Constraints
  1, 4, 5 bind Phase 31 (not this phase, but the parser must not foreclose them).
  Constraints 2, 3, 6 bind this phase directly:
  1. **No launch-time prediction (C1).** The stream-json mode must be always-on
     for the Claude adapter, or rolled out per-stage as an explicit sequencing
     choice — never selected by predicting whether a given stage will
     background. (Binds Phase 31's adapter switch; the parser this phase
     builds must work unconditionally, not only for "stages that background.")
  2. **Three independently landable plans (C2):** (i) a Claude stream-event
     parsing layer in `agent_result.rs` mirroring the existing Codex
     event-stream pattern — **this phase's 30b**; (ii) a pipe-owning monitor
     path replacing the `sh` script for the Claude adapter — **Phase 31**;
     (iii) the adapter flag/argv switch — **Phase 31**.
  3. **The parse layer mostly dies under JSONL capture (H1) — verified against
     source this session, see `## Common Pitfalls`.** `parse_devflow_result`'s
     whole-doc path, `claude_session_id`, `detect_claude_rate_limit`, and
     envelope-failure detection all fail on JSONL. `blocking_human_checkpoint_reported`
     survives only by accident and gains a false-positive surface (the stream
     echoes the full prompt back, so prompt text that merely *documents* a gate
     reads as a live one). The 4000-char tail window is smaller than a single
     stream `result` event line. **Multiple `result` events per process is the
     new normal — last-result semantics, never first-result.**
  4. **Stdin-close must gate on marker AND drained task set (H2).** Binds Phase
     31's monitor. The stream provides `background_tasks_changed` events that
     drain to `[]` — close only on marker-in-a-top-level-`result` AND empty
     task set. Close-with-pending-tasks behavior is untested/undefined.
  5. **Idle timeout, not wall-clock; the timeout writes an authoritative
     result (H3).** Binds Phase 31's monitor.
  6. **Evidence gaps to close in-phase (M1-M4):** exit-timing re-measurement +
     archival (**30d**, M1); task-notification behavior is unpinned CLI
     behavior on `claude_code_version 2.1.220` — smoke-detect or pin (M2, out
     of this phase's explicit unit list but the parser should not assume a
     specific CLI version); near-simultaneous completions untested (M3, out of
     scope — assigned to Phase 31 per ROADMAP's "Next phase" section); **the
     deciding production-environment test — run the v3 harness once through
     `spawn_monitor` itself — is 30c (M4)**.
- **Operator sizing cap: no phase exceeds M.** This phase is scoped to fit that
  cap (30a already closed; 30b is the only substantial unit; 30c/30d are S).
- **30c does NOT modify `spawn_monitor`.** It is a decision-gate experiment,
  not an implementation change to the shipped monitor path.
- **If 30c refutes task-notification delivery in the production launch
  context, Phase 31 is cancelled before it is planned** and 999.64 re-scopes
  around a rejected-options table. This is a real branch point the planner
  must represent (e.g., as a `checkpoint:decision` or explicit conditional
  next-step note in 30c's plan), not silently assume success.

### Claude's Discretion

- Exact function names/signatures for the new Claude stream parser, as long as
  they mirror the existing Codex pattern's shape (pure functions, JSONL
  line-split, `filter_map` parse, reverse-iterate for last/terminal event).
- Where the "real capture" test fixtures physically live inside the crate (see
  `## Common Pitfalls` — Pitfall 5 — for why they must live inside the crate,
  not be `include_str!`'d from `.planning/`).
- Whether 30d's re-measurement is a new small harness script or an extension
  of `run_experiment_v3.py`.
- Exact shape of 30c's standalone environment-replication harness (see
  `## Architecture Patterns / Pattern 3`).

### Deferred Ideas (OUT OF SCOPE)

- **999.65** (loop-back issues `--gaps-only` on a mid-arc phase) — separate
  phase, sequenced after 999.64.
- **999.66** (`consecutive_failures` accumulates on healthy progress) —
  separate phase, sequenced after 999.64.
- **999.46** (leaked fixture processes) — hygiene, not reliability, not
  blocking.
- **Anything release-related** (the original Phase 30 release-cut automation
  scope is shelved as a spike, not this phase).
- **The pipe-owning monitor rewrite, the always-on adapter switch, and the
  live Phase 29 wave-2 re-run acceptance test** — all Phase 31, gated on 30c.
</user_constraints>

## Summary

This phase has no unknowns about *what* Claude Code's `stream-json` protocol looks
like under the shape DevFlow will actually hit it (two concurrent background
subagents, the Phase 29 wave-2 shape) — that work is done and archived at
`.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/`
(`raw_output.jsonl` = invalid v1, `raw_output_v2.jsonl` = v2, `raw_output_v3.jsonl` =
v3, 54 lines, 3 `result` events, 3 `init` events, all one `session_id`). This
research catalogues every event type actually present in those captures (Step 1 of
the phase's research focus) and cross-references it against the **live source** of
`crates/devflow-core/src/agent_result.rs` (2964 lines) to confirm, line-by-line, that
the review's H1 finding is not speculation: `parse_devflow_result`,
`claude_session_id`, `detect_claude_rate_limit`, and `detect_claude_envelope_failure`
all parse the ENTIRE stdout as a single `serde_json::Value` document
(`serde_json::from_str(trimmed)` on the whole string) — a JSONL capture with 54 lines
and 3 independent JSON objects fails that parse outright (`serde_json::from_str`
requires the whole input to be one value; trailing content after the first `}` is a
hard error, not truncation).

The existing Codex adapter already solved an adjacent problem — a JSONL event
stream with a **different** schema (`thread.started`/`turn.*`/`item.*`) — via
`parse_codex_event_result` (agent_result.rs:551-612): filter/parse each line
independently, reverse-iterate to find the LAST decisive marker or terminal event,
defer (`None`) rather than guess when the terminal event is ambiguous. That is the
literal pattern to mirror for Claude's stream (`system`/`assistant`/`user`/`result`/
`rate_limit_event` schema), not a new design. `is_codex_event_stream`'s gate
(`thread.started` or `turn.*`) already correctly returns `false` for Claude JSONL, so
the two parsers will not collide by construction — no new dispatch logic is needed
beyond adding a `is_claude_event_stream` sibling gated on `type: "system", subtype:
"init"` (present in every capture, line 5/32/47 of v3) or on `type: "result"` with a
`session_id` field.

**Primary recommendation:** build `parse_claude_event_result` as a same-shape sibling
of `parse_codex_event_result` in `agent_result.rs`, wire it into `evaluate_layer1`'s
existing `.or_else()` cascade, and write its tests as literal inline strings copied
verbatim (with a source citation comment) from the archived v2/v3 JSONL lines —
matching the crate's existing test convention exactly (no `fixtures/` directory
exists anywhere in the workspace; `include_str!` reaching outside the crate root
would break `cargo publish` packaging, see Pitfall 5). Treat 30c as a real go/no-go
gate, not a formality: its environment-replication harness must hold its own stdin
pipe open (unlike production's `Stdio::null()`) while otherwise matching
`spawn_monitor`'s process shape (detached, `hermetic_command`-scrubbed env, stderr
redirected to a separate file) — the goal is isolating "does the CLI still deliver
`task-notification`-origin results outside an interactive session," not testing the
close-on-drain logic (that's Phase 31).

## Architectural Responsibility Map

This is a Rust CLI/library project (`devflow-core` + `devflow-cli`), not a web
application — the template's Browser/SSR/API/CDN/DB tiers do not map cleanly.
Substituted with this project's actual process/library boundaries.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Claude stream-json event parsing (30b) | `devflow-core` library (`agent_result.rs`) | — | Pure functions, no I/O beyond the string in; mirrors the existing Codex parser's placement in the same module |
| Fixture/evidence data for parser tests | `devflow-core` crate test module (in-file literals) | `.planning/phases/30-.../30a-evidence/` (source of truth, not directly compiled) | Must live inside the crate for `cargo publish` packaging (Pitfall 5); the `.planning/` copy remains the citable original |
| Production-environment go/no-go experiment (30c) | Standalone harness script (outside the crate, e.g. `.planning/phases/30-.../30c-harness.py` or `scripts/`) | `devflow-core::git::hermetic_command` (referenced for env-scrub parity, not invoked) | Explicitly must NOT modify `spawn_monitor` (locked decision) — it is a decision gate, not shipped code |
| Exit-timing re-measurement (30d) | Standalone harness script | — | Same rationale as 30c; produces an archived measurement artifact, not shipped code |
| Monitor pipe ownership / close-on-drain / idle timeout | `devflow-core::monitor` (`spawn_monitor`) | `devflow-cli::pipeline_launch` | **Phase 31 — out of scope for this phase's plans, do not implement here** |
| Adapter flag/argv switch (`--input-format stream-json`) | `devflow-core::agents::claude` (`ClaudeAgent::exec_command`) | — | **Phase 31 — out of scope for this phase's plans, do not implement here** |

## Standard Stack

No new external dependencies. This phase is additive parsing logic inside an
existing module using dependencies already present in `devflow-core`.

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde_json` | `1` (workspace-pinned, `Cargo.toml:22`) | Parse each JSONL line into `serde_json::Value`, mirroring every existing parser in this module | Already the sole JSON dependency of `agent_result.rs`; the Codex parser (line 551) uses the identical `.lines().filter_map(serde_json::from_str)` idiom this phase's parser should reuse verbatim |
| `serde` | `1` (workspace-pinned) | `AgentResult`/`AgentStatus`/`Verdict` (de)serialization, unchanged | Already in use; no new derive surface needed — the new parser constructs `AgentResult` the same way `parse_codex_event_result` does |

### Supporting
None. `thiserror` (already a dependency, `Cargo.toml:16`) is available if a new
error variant is warranted, but the existing pattern (`Option<AgentResult>`, no
`Result`, silent `None` on anything unparseable) is what every sibling function in
this module already does — **no new error type is standard here; deviating from
`Option`-returning parsers would be inconsistent with the module's own convention**,
verified by reading all six parsing functions (`parse_devflow_result`,
`detect_claude_rate_limit`/`detect_codex_rate_limit`, `detect_claude_envelope_failure`,
`claude_session_id`, `parse_codex_event_result`) — every one returns `Option<T>`,
never `Result<T, E>`.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled line-by-line `serde_json::Value` parsing (the existing/recommended approach) | A typed `serde` struct per event `type`/`subtype` (e.g. `#[serde(tag = "type")] enum ClaudeStreamEvent`) | More type safety, but the existing Codex parser deliberately stays untyped/`Value`-based to tolerate unknown fields and future schema additions without a compile break (`is_codex_event_stream` and `parse_codex_event_result` never fail closed on an unrecognized `type`). A typed enum risks `#[serde(other)]`-swallowing new event types silently, or hard-erroring on them — the untyped approach is the module's established risk posture and should not be silently upgraded without an explicit decision |
| `serde_json::Deserializer::from_str(...).into_iter::<Value>()` (streaming multi-value parser) | Manual `.lines()` split | The Codex parser already uses `.lines()`, and this project's captures are always newline-delimited (confirmed: every v2/v3 JSONL line is one complete object, no multi-line JSON values observed in 3 captures). Matching the existing idiom exactly is lower-risk than introducing a second JSONL-parsing strategy in the same file |

**Installation:** none — no `Cargo.toml` change required for 30b.

**Version verification:** `serde_json = "1"` already resolved at
`crates/devflow-core/Cargo.toml:20` (workspace-pinned via `serde_json.workspace =
true`); no action needed. `claude` CLI installed on this development machine
resolves to `2.1.220` (`claude --version` — matches the `claude_code_version` field
recorded in every archived capture's `init` event, so the local dev environment is
representative of the captured evidence) `[VERIFIED: local shell, claude --version]`.

## Package Legitimacy Audit

**Not applicable — this phase installs no new external packages.** It adds pure
Rust functions to an existing module using dependencies already present and
workspace-pinned (`serde_json`, `serde`). No `Cargo.toml` edits are anticipated for
30b/30c/30d.

**Packages removed due to [SLOP] verdict:** none — no packages evaluated, none needed.
**Packages flagged as suspicious [SUS]:** none.

## Architecture Patterns

### System Architecture Diagram

Current state (what exists today) → target state after 30b (parser only; monitor
launch mechanics unchanged in this phase):

```
                    TODAY (Layer 1 dies silently under JSONL)
┌──────────────────────────────────────────────────────────────────────┐
│  claude -p ... --output-format json   (single-document envelope)      │
│         │                                                              │
│         ▼                                                              │
│  .devflow/phase-NN-stdout  (one JSON object, whole-doc parseable)      │
│         │                                                              │
│         ▼                                                              │
│  evaluate_layer1()  agent_result.rs:660                                │
│    detect_claude_rate_limit → detect_claude_envelope_failure →         │
│    parse_devflow_result → parse_codex_event_result (gate: false) →     │
│    detect_codex_rate_limit                                             │
│  ── all single-doc-shaped, all correctly handle TODAY'S envelope ──    │
└──────────────────────────────────────────────────────────────────────┘

                 AFTER PHASE 31 (out of scope here, shown for context)
┌──────────────────────────────────────────────────────────────────────┐
│  claude -p --input-format stream-json --output-format stream-json      │
│         │   (JSONL: init, assistant, user, result x N,                 │
│         │    background_tasks_changed, task_started, task_notification,│
│         │    task_updated, task_progress, rate_limit_event,            │
│         │    thinking_tokens)                                          │
│         ▼                                                              │
│  .devflow/phase-NN-stdout  (multi-line JSONL, 3+ result events seen)   │
│         │                                                              │
│         ▼                                                              │
│  evaluate_layer1()  ── NEW: parse_claude_event_result inserted ──      │
│    detect_claude_rate_limit (dies on JSONL, falls through) →           │
│    detect_claude_envelope_failure (dies on JSONL, falls through) →     │
│    parse_devflow_result (dies on JSONL — tail scan cuts mid-object) →  │
│    ★ parse_claude_event_result (NEW, THIS PHASE'S 30b) ★               │
│        - gate: is_claude_event_stream (system/init present)            │
│        - discriminate parent_tool_use_id == null (top-level only)      │
│        - reverse-iterate result events → LAST result wins              │
│        - extract DEVFLOW_RESULT marker from LAST result's text         │
│        - extract session_id from LAST-seen init (same across turns)    │
│        - rate_limit_event handling (new top-level event type)          │
│    → parse_codex_event_result (gate: false, unaffected) →              │
│    detect_codex_rate_limit                                             │
└──────────────────────────────────────────────────────────────────────┘
        │
        ▼  (Phase 31 only — monitor rewrite, NOT this phase)
  spawn_monitor's sh script (stdin(Stdio::null())) replaced by a
  pipe-owning monitor that closes stdin only on
  marker-in-top-level-result AND background_tasks_changed drained to []
```

### Recommended Project Structure

No new files/directories for source. Test fixtures (literal strings) live inline in
the existing test module, matching the Codex parser's convention exactly:

```
crates/devflow-core/src/
└── agent_result.rs         # add parse_claude_event_result + is_claude_event_stream
                             # next to parse_codex_event_result/is_codex_event_stream
                             # (same file, same module — no new file)
    └── mod tests { ... }   # add tests as literal `concat!` string constants,
                             # copied verbatim from 30a-evidence/raw_output_v3.jsonl
                             # with a doc comment citing the source line numbers
```

For 30c/30d (experiments, not shipped code — do not put these under `crates/`):

```
.planning/phases/30-keep-the-session-alive-past-turn-end/
├── 30a-evidence/            # already exists — do not modify
├── 30c-<name>.py            # NEW: env-replication harness, does not touch monitor.rs
├── 30c-evidence/            # NEW: raw JSONL + measured verdict from the 30c run
├── 30d-<name>.py            # NEW: re-measurement harness (may extend run_experiment_v3.py)
└── 30d-evidence/            # NEW: archived exit-timing measurements, including the
                              # untested close-with-pending-tasks case
```

### Pattern 1: Mirror the Codex event-stream parser shape exactly

**What:** `parse_codex_event_result` (agent_result.rs:551-612) is the load-bearing
precedent 30b must structurally copy: (1) split stdout into lines, filter empty,
`filter_map` each through `serde_json::from_str::<Value>`, collecting only what
parses; (2) a cheap gate function (`is_codex_event_stream`) that returns `false`
fast when the capture isn't this adapter's shape, so the cascade correctly falls
through to the next parser; (3) reverse-iterate (`.iter().rev().find_map(...)`) to
implement **last-marker/last-result-wins** semantics; (4) return `None` (defer to
Layer 2) rather than guessing, when the terminal signal is ambiguous.

**When to use:** Any time a new adapter's native event-stream format needs a
decisive parser slotted into `evaluate_layer1`'s `.or_else()` chain.

**Example (existing Codex code to mirror — not new):**
```rust
// Source: crates/devflow-core/src/agent_result.rs:523-529, verified this session
fn is_codex_event_stream(events: &[serde_json::Value]) -> bool {
    events.iter().any(|v| {
        v.get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|t| t == "thread.started" || t.starts_with("turn."))
    })
}
```
The Claude sibling gate should key off `type: "system", subtype: "init"` (present
in EVERY archived capture, lines 5/32/47 of `raw_output_v3.jsonl`) — this is a
stronger, earlier signal than waiting for a `result` event, and lets the parser
short-circuit even mid-stream.

### Pattern 2: Discriminate `parent_tool_use_id == null` before trusting ANY event

**What:** Every event in the v2/v3 captures — `assistant`, `user`, and (per the v1
pitfall) even top-level-shaped events — can carry a non-null `parent_tool_use_id`
when it is narration **forwarded from a subagent**, not the orchestrator itself.
v1's harness (`.../30a-evidence/raw_output.jsonl`, line 12) proved this concretely:
its turn-detection heuristic mistook a `parent_tool_use_id`-set `assistant` event
for orchestrator resumption and produced an invalid verdict.

**When to use:** Any code that inspects `assistant`/`user` event content
(specifically the checkpoint-gate text scan, Pitfall 3 below). `result` events in
the archived captures never carry `parent_tool_use_id` — it is absent from every
`result` event's key set in all three captures (`[VERIFIED: 30a-evidence/raw_output_v3.jsonl`
lines 19, 37, 54 — `parent_tool_use_id` not present in `result`'s key list]`), so
this discrimination matters specifically for `assistant`/`user` events, not for
locating the terminal `result`.

**Example:**
```rust
// Confirmed present/absent pattern, this session (30a-evidence/raw_output_v3.jsonl):
// line  6 assistant parent_tool_use_id=null   <- top-level orchestrator turn
// line 11 assistant parent_tool_use_id=toolu_01FVk15W...  <- subagent-forwarded
let is_top_level = event
    .get("parent_tool_use_id")
    .map(|v| v.is_null())
    .unwrap_or(true); // key absent (e.g. on `result`/`init`) = top-level
```

### Pattern 3: 30c's environment-replication harness (does NOT touch `spawn_monitor`)

**What:** `spawn_monitor` (`crates/devflow-core/src/monitor.rs:45-179`) launches the
agent via `hermetic_command("sh", workdir_path).arg("-c").arg(&script)...`, with
`env_remove` applied for every var in `REPO_LOCAL_GIT_VARS`/`ALSO_REDIRECTING_GIT_VARS`
(`git.rs:87-94`), and all three of stdin/stdout/stderr set via `Stdio::null()`
(`monitor.rs:171-173`) — the agent's actual stdout/stderr are captured by the `sh`
script's own `>`/`2>` redirection to files, not by the Rust `Command`'s pipes.
Running the v3 harness "through `spawn_monitor` itself" **literally** (unmodified)
is not possible without breaking the harness — a harness that needs to hold `stdin`
open to observe post-`result` events cannot do so against a `Command` configured
with `stdin(Stdio::null())`. 30c must therefore be a **new standalone harness**
that replicates `spawn_monitor`'s environment characteristics (detached process,
`hermetic_command`-equivalent env scrubbing, stderr separated to its own file, no
TTY, launched via `sh -c` the same way) while keeping its OWN Python-side stdin
pipe open (as `run_experiment_v3.py` already does) — this isolates the ENVIRONMENT
variable (interactive session vs. detached production process) as the thing under
test, without conflating it with the pipe-ownership rewrite that is Phase 31's job.

**When to use:** 30c only. Do not wire this harness's launch mechanics into
`monitor.rs` — that coupling is exactly what the locked decision "30c does NOT
modify it" forbids.

**Example (env-scrub list to replicate, read from source):**
```rust
// Source: crates/devflow-core/src/git.rs — REPO_LOCAL_GIT_VARS / ALSO_REDIRECTING_GIT_VARS
// (grep both const names in git.rs to get the exact current list; do not
// hardcode a stale copy into the 30c harness — re-read at implementation time)
```

### Anti-Patterns to Avoid
- **Trusting the FIRST `result` event as the process's outcome:** every archived
  capture has 3 `result` events (v3) or 2 (v2); only the LAST reflects the final
  state. The existing single-document parsers (`parse_devflow_result` et al.) were
  correct to treat their one `result` as authoritative — under JSONL that
  assumption silently breaks unless explicitly reversed to last-wins, matching
  `parse_codex_event_result`'s own convention.
- **Widening `text_reports_human_gate`'s scan to raw multi-event JSONL text without
  first excluding echoed `user` events:** the raw stream echoes the operator's
  full input prompt back as a `user` event's content. If a prompt happens to
  *document* the `**Gate:** \`blocking-human\`` rendering (as this very research's
  own prompt does, and as GSD's `execute-phase.md`/`gsd-executor.md` reference
  material does), a naive full-text scan reads that documentation as a live gate
  declaration. `blocking_human_checkpoint_reported` currently scans raw stdout
  UNBOUNDED (not line/tail-limited) — see Pitfall 3.
- **Assuming a new `system`/`init` event means a new session:** all 3 `init`
  events in `raw_output_v3.jsonl` (lines 5, 32, 47) carry the IDENTICAL
  `session_id`. Do not key session continuity off "have we seen an `init`
  event" — key it off the `session_id` value itself.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSONL line parsing | A custom streaming/incremental JSON reader | `.lines().filter_map(serde_json::from_str::<Value>)` (already the Codex parser's exact idiom, `agent_result.rs:552-556`) | Every observed capture (v1/v2/v3, 12/32/54 lines) is strictly newline-delimited, one complete JSON value per line — no multi-line values were ever observed. A more general streaming parser (`serde_json::Deserializer::from_str(...).into_iter()`) solves a problem that does not exist in this data and would be the ONLY such usage in the file, inconsistent with the sibling parser |
| Detecting "is this a Claude stream" vs "is this a single-doc envelope" | A byte-sniffing heuristic (e.g. "does line 2 exist") | Reuse `is_codex_event_stream`'s proven shape: parse all lines first, then check for a discriminating field (`type:"system", subtype:"init"`) | The Codex sibling already solved exactly this dispatch problem for a different adapter; the risk of a bespoke heuristic silently misclassifying a single-doc envelope as a stream (or vice versa) is exactly what the existing gate-function pattern was built to avoid |

**Key insight:** This phase's core technical risk is not novel — it's a second
instance of a problem `agent_result.rs` already solved once for Codex. The research
finding that matters most for planning is *how closely 30b's implementation should
copy the existing Codex code's shape*, not designing something new.

## Common Pitfalls

### Pitfall 1: Every single-document Claude parser silently returns `None` under JSONL — verified line-by-line
**What goes wrong:** `extract_json_result_text` (agent_result.rs:232-239),
`claude_session_id` (agent_result.rs:266-273), `detect_claude_rate_limit`
(agent_result.rs:166-180), and `detect_claude_envelope_failure`
(agent_result.rs:368-405) all begin with `let trimmed = stdout.trim(); if
!trimmed.starts_with('{') { return None; }` followed by `serde_json::from_str(trimmed)`
on the ENTIRE string.
**Why it happens:** `serde_json::from_str` requires the whole input to be exactly
one JSON value — any trailing content after the first closing `}` is a hard parse
error (`Error("trailing characters", ...)`), which these functions all map to
`.ok()?` → `None`. A 54-line JSONL capture (`raw_output_v3.jsonl`) starts with `{`
(the first `system`/`init` line) but is NOT one JSON document, so every one of
these four functions returns `None` on it today, unconditionally.
**How to avoid:** 30b must not "fix" these four functions in place — mirror the
Codex precedent instead: add a NEW sibling function
(`parse_claude_event_result`/`is_claude_event_stream`) inserted into
`evaluate_layer1`'s cascade, exactly where `parse_codex_event_result` already sits.
The four existing single-doc functions remain correct for the CURRENT
`--output-format json` envelope (still in use — `agents/claude.rs:26-27`, unchanged
until Phase 31) and must not be modified by this phase.
**Warning signs:** Any test that feeds a full multi-line JSONL capture into
`parse_devflow_result`/`claude_session_id`/`detect_claude_rate_limit` and asserts a
non-`None` result — those functions are, by source-verified design, single-document
only; a passing test there for JSONL input would indicate a mistaken edit to those
functions rather than the new sibling.

### Pitfall 2: The 4000-char tail scan can't reach into a multi-KB `result` event
**What goes wrong:** `parse_marker_lines` (agent_result.rs:619-647) takes only the
LAST 4000 characters of `stdout`, then splits by line and looks for a bare
`DEVFLOW_RESULT:`-prefixed line. A single `result` event line in the archived
captures already runs several hundred bytes (usage/modelUsage/cache stats are
verbose — see line 19/37/54 dumps, this session) and DevFlow's real Plan/Code
stages produce much longer `result.result` text than this experiment's one-line
acknowledgments. A tail window sized for single-document envelopes can land
mid-object in a JSONL capture, and even if it lands cleanly, the marker text is
JSON-escaped inside a `"result": "..."` string value — it will never appear as a
bare line starting with `DEVFLOW_RESULT:`.
**Why it happens:** The tail-scan's design assumption (marker is a plain line
in mostly-plain-text stdout, escaped-and-unwrapped once via
`extract_json_result_text`) does not generalize to "marker is JSON-escaped inside
the LAST of several `result` events in a JSONL stream."
**How to avoid:** 30b's parser must locate the LAST top-level `result` event first
(via the reverse-iterate pattern, Pattern 1), extract ITS `result` field text (already
unescaped once decoded from JSON — no manual unescaping needed, unlike the
`extract_json_result_text`/tail-scan combination), and run the EXISTING
`parse_marker_lines` on that isolated text, not on the raw multi-event stdout.
This reuses `parse_marker_lines` unchanged — it just needs a correctly-scoped input.
**Warning signs:** A DEVFLOW_RESULT marker present in the archived captures'
final `result.result` field (none of the current 3 captures contain one — they're
all pure harness-instruction text, not real GSD phase output) not being found by
whatever 30b builds; must be caught by a synthetic test since no archived capture
happens to contain a real marker.

### Pitfall 3: `blocking_human_checkpoint_reported` gains a false-positive surface under JSONL (review constraint 3)
**What goes wrong:** `blocking_human_checkpoint_reported`
(agent_result.rs:459-466) calls `text_reports_human_gate` on the RAW, UNBOUNDED
stdout string (not tail-limited, not line-split) — this is deliberate today, because
under the current single-document envelope the only place gate text can appear is
inside the one `result.result` field. Under stream-json, the raw multi-event text
also contains the full original PROMPT, echoed back verbatim inside a `user` event
(`raw_output_v3.jsonl` lines 10/16/27/39/42 are all `type: "user"`). Any prompt that
*documents* the gate rendering (a plan file quoting `**Gate:** \`blocking-human\``,
or — concretely — this very RESEARCH.md's own text, if it were ever fed back into a
prompt) reads as a live gate declaration to a text-substring scan that doesn't know
which event it's inside.
**Why it happens:** `text_reports_human_gate`'s case-insensitive substring scan for
`"gate"` was designed against text that could only originate from the agent's own
final report. Stream-json breaks that invariant — echoed input is now
indistinguishable, textually, from agent-authored output.
**How to avoid:** Scope the gate scan to `assistant`/`result` event text only,
explicitly excluding `type: "user"` events (which are always either the operator's
original prompt or a `task_notification`'s summary re-injected as user-role
content — line 27/39/42 pattern). Apply Pattern 2's `parent_tool_use_id == null`
discrimination on top, so subagent-forwarded text (which could also echo a prompt)
is excluded too.
**Warning signs:** A checkpoint auto-decide firing (or the resume-ceiling counter
incrementing) on a stage whose prompt merely discusses checkpoints without the
agent actually declaring one — this is the live failure mode the review is warning
about, and it degrades silently since D-02's design already treats a false positive
here as "bounded but not zero-cost" (resume-ceiling consumption), not something
that surfaces as an error.

### Pitfall 4: `cargo test --exact` with a bare test name silently matches nothing (project-standing pitfall, reconfirmed)
**What goes wrong:** `cargo test <name> --exact` where `<name>` is not the full
`module::path::test_name` matches zero tests and STILL exits 0 — this looks like a
green run and is not. Reconfirmed this session:
`cargo test -p devflow-core --lib agent_result::tests::parse_success_marker` correctly
reports `test result: ok. 1 passed; 0 failed; ... 454 filtered out`. Any invocation
that reports `0 passed` (with any filter count) ran nothing, not "all pass."
**Why it happens:** Cargo's test filtering silently accepts a filter that matches
no tests; it is not an error condition by design.
**How to avoid:** Every verification step in 30b's plan that runs a specific new
test by name MUST assert on `N passed; 0 failed` in the output, where N ≥ 1 — never
just "exit code 0." The package name for `cargo test -p` in this workspace is
`devflow-core` (the crate this phase edits) or `devflow` (the CLI binary crate,
confirmed via `crates/devflow-cli/Cargo.toml:2` — NOT `devflow-cli`).
**Warning signs:** A verification step or SUMMARY.md claiming a specific new test
passed without quoting the `N passed` line from the actual command output.

### Pitfall 5: `include_str!` reaching outside the crate root breaks `cargo publish` packaging
**What goes wrong:** The phase's own research focus suggests "test fixtures are the
archived v2/v3 raw logs in `30a-evidence/`" — the naive way to wire that up is
`include_str!("../../../.planning/phases/30-.../30a-evidence/raw_output_v3.jsonl")`
from inside `crates/devflow-core/src/agent_result.rs`. This compiles and passes
tests locally (Rust's `include_str!` has no crate-boundary restriction for
`cargo build`/`cargo test`), but `devflow-core` is published to crates.io
(confirmed: `crates/devflow-core/Cargo.toml` has package metadata for publishing,
and this project's own history — STATE.md — records "`devflow-core` published
first... then `devflow`" as a standing release step). `cargo package`/`cargo
publish` builds from an isolated tarball containing only files inside the crate
root; a path reaching up through `../../../.planning/` will not exist in that
tarball and the packaged build will fail — a failure mode that never reproduces
locally (the `.planning/` directory is right there in a normal checkout) and would
only surface at the next real release.
**Why it happens:** No workspace member's `Cargo.toml` sets `package.include` or
`package.exclude` (confirmed — grepped both crates' `Cargo.toml`s, no matches), so
default packaging rules apply: everything under the crate root not gitignored, and
nothing outside it.
**How to avoid:** Copy the needed excerpts as literal Rust string constants inside
`agent_result.rs`'s test module (matching the existing `concat!`-string convention
used by every Codex parser test, e.g. `codex_event_stream_parses_turn_failed`,
agent_result.rs:1730-1741) — with a doc comment citing the exact source file and
line numbers in `.planning/phases/30-.../30a-evidence/raw_output_v3.jsonl`, so the
fixture remains traceably "real capture, not synthetic" without crossing the crate
boundary. Alternatively, a `crates/devflow-core/tests/fixtures/` directory (there is
no established precedent for this in the workspace, but it would stay inside the
crate root and be publish-safe) — Claude's discretion which of the two the plan
picks, but NOT `include_str!` from `.planning/`.
**Warning signs:** A `cargo publish --dry-run` (or the project's actual release
script) failing with a "file not found" error referencing a `.planning/` path — this
would be the first time such a failure could occur, since no prior parser work in
this file has ever referenced anything outside `crates/`.

### Pitfall 6: An extra `local_bash`-typed `task_started`/`task_notification` pair appears per `Task`-tool child — undocumented, not yet explained
**What goes wrong:** In `raw_output_v3.jsonl`, each of the two `Task`-tool-spawned
subagents produces not one but effectively two tracked-task lifecycles: a
`local_agent`-typed `background_tasks_changed`/`task_started` pair (lines 8-9, 13-14)
matching the `Task` tool's own `tool_use_id`, AND a separate `local_bash`-typed
`task_started`/`task_notification` pair (lines 24, 26; 25, 38) with a DIFFERENT
`task_id` and no corresponding `background_tasks_changed` entry, whose
`task_notification.summary` is a short description ("Sleep 10s then write signal
file") rather than the subagent's own final report. This is not documented in the
6-point review's constraints and was not called out in the phase's research focus.
**Why it happens:** Unknown — plausibly the CLI internally tracks the subagent's
OWN `Bash` tool call (`sleep 10 && ...`) as a second, nested background task
alongside the outer `Task` dispatch, surfacing both to the top-level stream. This
is a genuine gap in current understanding, not resolved by re-reading the captures
further.
**How to avoid:** 30b's `background_tasks_changed`-drain check (needed by Phase 31,
constraint 4) must be built against the OUTER `local_agent`-typed tasks only —
those are what `background_tasks_changed`'s array actually tracks (confirmed:
`raw_output_v3.jsonl` line 44's empty-array event follows the SECOND child's
completion and only ever listed `local_agent`-typed entries in its non-empty form,
lines 8 and 29). The `local_bash` pair appears to be informational/nested and
should not gate anything on its own. Flag this explicitly as an assumption
(see `## Assumptions Log`) rather than silently building drain-detection logic that
happens to work on 2 captures without understanding why.
**Warning signs:** A drain-detection implementation (Phase 31, not this phase) that
waits for `local_bash` tasks to also disappear from some tracked set that doesn't
actually exist in the `background_tasks_changed` payload — there is no evidence
`local_bash` tasks are ever listed in `background_tasks_changed.tasks`.

## Code Examples

Verified patterns from the actual archived captures and live source (all cited
inline; none of this is Context7/external-docs sourced — this phase's domain is
this project's own evidence, not a third-party library).

### Catalogue of stream event types actually present (research focus item 1)

Full type/subtype inventory from `raw_output_v3.jsonl` (54 lines), cross-checked
against `raw_output.jsonl` (12 lines, v1-invalid) and `raw_output_v2.jsonl` (32
lines) this session:

| `type` | `subtype` | Lines (v3) | Key fields observed | Notes |
|--------|-----------|------------|---------------------|-------|
| `system` | `hook_started` | 1, 2 | — | Harness startup noise, not decision-relevant |
| `system` | `hook_response` | 3, 4 | — | Harness startup noise |
| `system` | `init` | 5, 32, 47 | `session_id`, `claude_code_version`, `cwd`, `tools`, `agents`, `skills` | **Same `session_id` in all 3** — confirms "same live session" claim; `claude_code_version: "2.1.220"` pinned in every one |
| `assistant` | — | 6,7,11,12,17,18,20,22,28,35,36,40,43,50,51,53 | `message.content[]` (text/tool_use blocks), `parent_tool_use_id` | Discriminate top-level vs. subagent-forwarded via `parent_tool_use_id` (Pattern 2) |
| `user` | — | 10,16,27,39,42,52 | `message.content`, `parent_tool_use_id` | Prompt echo risk (Pitfall 3); can carry non-null `parent_tool_use_id` too |
| `system` | `background_tasks_changed` | 8,13,29,44 | `tasks: [{task_id, task_type, description}]` | Empty array at line 44 = fully drained (Phase 31's close-gate signal) |
| `system` | `task_started` | 9,14,24,25 | `task_id`, `tool_use_id`, `description`, `subagent_type`?, `task_type` (`local_agent` or `local_bash`) | See Pitfall 6 — two task_type flavors observed per child |
| `system` | `task_notification` | 26,31,38,46 | `task_id`, `tool_use_id`, `status`, `output_file`, `summary`, optional `usage{total_tokens,tool_uses,duration_ms}` | `local_agent` notifications carry `usage`; `local_bash` ones don't |
| `system` | `task_updated` | 30,45 | `task_id`, `patch:{status,end_time}` | Epoch-ms `end_time` |
| `result` | `success` | 19,37,54 | `origin.kind` (absent on line 19, `"task-notification"` on 37/54), `num_turns`, `session_id`, `result`, `stop_reason`, `usage`, `total_cost_usd` | **The decisive event.** `origin.kind` presence is the discriminator for "was this turn triggered by a background completion" |
| `rate_limit_event` | — (top-level `type`, not nested under `system`) | 15 | `rate_limit_info:{status,resetsAt,rateLimitType,overageStatus,overageDisabledReason,isUsingOverage}` | **Top-level `type`, NOT `type:"system", subtype:"rate_limit_event"`** — must be matched on `type` alone |
| `system` | `thinking_tokens` | 33,34,48,49 | — | Not called out in the ROADMAP's research focus list; appears twice after every `init` recurrence; likely safely ignorable but undocumented — flag as an open question, not silently assumed benign |

`parent_tool_use_id` discrimination, confirmed present/absent pattern
(`[VERIFIED: 30a-evidence/raw_output_v3.jsonl, this session's python parse]`):
lines 6,7,10,12,16,17,35,36,50,51,52,53 → `null` (top-level); lines 11,18,20,22,27,
28,39,40,42,43 → a `toolu_...` string (subagent-forwarded). v1's invalidating bug
(`raw_output.jsonl` line 12) had exactly this shape misread as orchestrator
resumption.

### Existing Codex pattern to mirror (source, not new)
```rust
// Source: crates/devflow-core/src/agent_result.rs:551-612, verified this session
fn parse_codex_event_result(stdout: &str) -> Option<AgentResult> {
    let events: Vec<serde_json::Value> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();

    if !is_codex_event_stream(&events) {
        return None;
    }
    // ... reverse-iterate for last marker, then last terminal event ...
}
```

### Existing envelope-failure detector's single-doc assumption (why it dies under JSONL)
```rust
// Source: crates/devflow-core/src/agent_result.rs:368-373, verified this session
fn detect_claude_envelope_failure(stdout: &str) -> Option<AgentResult> {
    let trimmed = stdout.trim();
    if !trimmed.starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).ok()?; // FAILS on JSONL:
    // "trailing characters" — the whole multi-line capture is not one JSON value.
    ...
}
```

### Test convention to match (existing, not new)
```rust
// Source: crates/devflow-core/src/agent_result.rs:1730-1741, verified this session
#[test]
fn codex_event_stream_parses_turn_failed() {
    let stdout = concat!(
        "{\"type\":\"thread.started\",\"thread_id\":\"t1\"}\n",
        "{\"type\":\"turn.started\"}\n",
        "{\"type\":\"item.started\",\"item\":{}}\n",
        "{\"type\":\"turn.failed\",\"error\":{\"message\":\"sandbox denied write\"}}\n",
    );
    let result = parse_codex_event_result(stdout).unwrap();
    assert_eq!(result.status, AgentStatus::Failed);
    assert_eq!(result.reason.as_deref(), Some("sandbox denied write"));
}
```
30b's tests should follow this literal-`concat!`-string shape, but the string
CONTENT should be copied verbatim (trimmed to the minimum necessary lines) from the
real archived captures rather than hand-authored, per the phase's own instruction
that fixtures be "real captures, not synthetic" — cite the source line numbers in a
doc comment above each test.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| Single-document `--output-format json` envelope, one `result` per process, whole-doc `serde_json::from_str` parsing | `--input-format stream-json --output-format stream-json`, JSONL, multiple `result` events per process, `origin.kind: "task-notification"` marking background-completion-triggered turns | Not yet changed in this codebase — `agents/claude.rs:26-27` still emits `--output-format json` today; this phase's research is preparation for a change that lands in **Phase 31**, gated on 30c | The entire Layer-1 parsing cascade in `agent_result.rs` needs the new sibling parser this phase builds BEFORE Phase 31 can safely flip the adapter's argv, or Layer 1 goes dark for every Claude-driven stage (silent fallback to Layer 2's coarser exit-code+commit heuristic, losing verdict/session_id/rate-limit/checkpoint signal fidelity) |

**Deprecated/outdated:** Nothing in this project is being deprecated by this phase —
the single-document envelope parsers remain correct and necessary for as long as
`agents/claude.rs` still launches with `--output-format json` (unchanged until
Phase 31's adapter switch).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `task-notification`-origin `result` events reliably fire for EVERY background completion in DevFlow's actual production launch context (detached, scrubbed env, no TTY) — not just inside an interactive Claude Code session | Summary, Pattern 3 | This is EXACTLY the question 30c exists to answer and is explicitly unresolved (review finding M4) — if false, Phase 31 is cancelled per the locked decision, and 30b's parser (still useful for the single-document/no-background case) would need re-scoping around whatever 30c's rejected-options table finds |
| A2 | The `local_bash`-typed `task_started`/`task_notification` pair (Pitfall 6) is purely informational and never needs to be tracked for drain-completeness — only `local_agent`-typed entries in `background_tasks_changed.tasks` matter | Pitfall 6 | If wrong, a Phase 31 close-gate built only on `local_agent` drain could close stdin while a `local_bash`-tracked internal operation is still pending, potentially reproducing a narrower version of the original orphaning bug |
| A3 | `type: "thinking_tokens"` events (v3 lines 33,34,48,49) are safe to ignore/pass through without special handling in the new parser | Architecture Patterns catalogue | Unlikely to matter for status detection, but an untriaged event type is always a risk that some future capture carries decision-relevant data under it; low risk, not verified against any documentation because none exists for this CLI internal event |
| A4 | `claude_code_version: "2.1.220"` behavior (task-notification delivery, event schema) is representative of the version DevFlow will actually run against in production, since the locally installed `claude` CLI matches this exact version | Standard Stack | If the operator's or CI's Claude CLI version drifts before Phase 31 ships, the event schema/behavior this phase's parser and 30c's experiment are built against could silently change — review finding M2 (smoke-detect or pin the CLI version) is explicitly assigned to Phase 31, not resolved here |

**If this table is empty:** N/A — see above; four assumptions carried forward,
consistent with the review's own explicit flags (M2, M4) plus two new findings from
this session's fresh read of the captures (A2, A3).

## Open Questions (RESOLVED)

1. **Does `local_bash`-typed task tracking (Pitfall 6) ever appear in
   `background_tasks_changed`, under a different shape than observed in these 2
   captures?**
   - What we know: In both v2 and v3, only `local_agent`-typed entries ever appear
     in a `background_tasks_changed.tasks` array; `local_bash` entries only ever
     appear via `task_started`/`task_notification`, never via
     `background_tasks_changed`.
   - What's unclear: Whether this is a stable CLI invariant or an artifact of this
     specific experiment's simple `sleep N` shape — real GSD plan/code work
     involves many more `Bash` tool calls per subagent, any of which could
     plausibly generate similar internal tracking.
   - **RESOLVED: deferred to Phase 31, not silently dropped.** 30b's parser does
     not need to resolve this (it only needs `local_agent` drain for the
     marker/last-result logic). Flagged explicitly for Phase 31's
     monitor-close-gate design, which DOES depend on drain-completeness. 30-04's
     Mode B harness additionally records every `local_bash` event it observes
     (RESEARCH.md Assumption A2), so Phase 31 inherits more evidence than this
     answer alone, not less.

2. **What does the CLI do on close-with-pending-background-tasks (review H2, M4's
   sibling)?**
   - What we know: Explicitly stated as untested/undefined by the review; none of
     v1/v2/v3 tested closing stdin while a background task was still running (all
     three waited for both signals before closing).
   - What's unclear: Everything — does the process hang, does it deliver a final
     truncated result, does the orphaned subagent's work simply vanish (the
     original bug, reproduced by construction)?
   - **RESOLVED: folded into this phase, not deferred.** Plan 30-04 Task 1 builds
     a Mode B trial that closes stdin early with a pending background task and
     archives the observed outcome (`exits_with_truncated_result`,
     `child_work_lost`, or the alternative it actually measures) alongside the
     Mode A exit-timing distribution. See 30-04-PLAN.md lines 108, 173, 180, 222.

3. **Is the `thinking_tokens` event type ever decision-relevant?**
   - What we know: Appears twice per `init` recurrence (right after each of the 3
     `init` events in v3), no field payload observed beyond `type`/`subtype`.
   - What's unclear: Whether it ever carries content this parser should extract.
   - **RESOLVED: treat as ignorable, by explicit decision, not oversight.** 30b's
     parser falls through on `thinking_tokens` (no match) unless a future capture
     shows otherwise — low risk, do not over-build against an unknown.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| `claude` CLI | 30c's environment-replication harness, 30d's re-measurement | ✓ | `2.1.220` (`claude --version`, matches every archived capture's `claude_code_version`) | — |
| `cargo`/`rustc` (workspace toolchain) | 30b's parser + tests | ✓ | `cargo 1.97.1`, `rustc 1.97.1` | — |
| `python3` | 30c/30d harness scripts (matching the existing `run_experiment_v3.py` convention) | Assumed available (existing v1/v2/v3 harnesses already ran successfully in this environment; not independently re-checked this session) | — | — |

**Missing dependencies with no fallback:** none identified.
**Missing dependencies with fallback:** none identified.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` (no external test framework; `#[cfg(test)] mod tests` inline, matching every existing module in this crate) |
| Config file | none — `[dev-dependencies] tempfile = "3"` in `crates/devflow-core/Cargo.toml` is the only test-support dependency; no `pytest.ini`/`jest.config`-equivalent exists for this Rust workspace |
| Quick run command | `cargo test -p devflow-core --lib agent_result::` (runs only this module's tests; confirmed working this session against `agent_result::tests::parse_success_marker` → `1 passed; 0 failed`) |
| Full suite command | `scripts/check.sh all` (= `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace --no-fail-fast`) — this is "the single definition of is this green?" per the script's own header comment, `[VERIFIED: scripts/check.sh:1-4]` |

### Phase Requirements -> Test Map

No REQ-IDs exist for this project's infrastructure phases (confirmed: no
`.planning/REQUIREMENTS.md` file exists in this repo at all). Mapped by unit
instead, per the ROADMAP entry's own convention:

| Unit | Behavior | Test Type | Automated Command | File Exists? |
|------|----------|-----------|---------------------|--------------|
| 30b | `parse_claude_event_result` extracts the LAST `result` event's marker/status, ignoring earlier ones | unit | `cargo test -p devflow-core --lib agent_result::tests::<new_test_name> -- --exact` (assert `1 passed`) | ❌ Wave 0 (new tests, same file) |
| 30b | `is_claude_event_stream` correctly gates false on the current single-doc envelope and true on JSONL, without colliding with `is_codex_event_stream` | unit | `cargo test -p devflow-core --lib agent_result::` | ❌ Wave 0 |
| 30b | Checkpoint-gate false-positive from prompt-echo (Pitfall 3) does NOT fire | unit (regression) | same module | ❌ Wave 0 — this is the single highest-value regression test given the review's constraint 3 |
| 30c | Task-notification delivery confirmed/refuted outside an interactive session | manual-only (experiment), archived evidence is the artifact | `python3 <30c-harness>.py`, then human/agent inspection of the resulting JSONL + verdict | ❌ Wave 0 — new harness script |
| 30d | Exit-timing (and close-with-pending-tasks, if folded in per Open Question 2) re-measured and archived | manual-only (experiment) | `python3 <30d-harness>.py` (may extend `run_experiment_v3.py`) | ❌ Wave 0 — new harness or extension |

### Sampling Rate
- **Per task commit:** `cargo test -p devflow-core --lib agent_result::` (fast, scoped)
- **Per wave merge:** `scripts/check.sh all` (full fmt+clippy+test gate — this is also
  what the repo's own pre-push hook enforces, per project memory: "the push path
  reproduces the /proc race" note refers to this same script running in-container)
- **Phase gate:** Full suite green before `/gsd-verify-work`; 30c/30d additionally
  require their archived evidence artifacts (JSONL + a written verdict) to exist on
  disk before the phase can be called complete — these are experiments, not just
  code changes, and their OUTPUT (the go/no-go answer for Phase 31) is the actual
  deliverable, not merely "the script ran."

### Wave 0 Gaps
- [ ] New tests in `crates/devflow-core/src/agent_result.rs`'s existing `mod tests`
      block covering `parse_claude_event_result`/`is_claude_event_stream` — no
      new test file needed, extends the existing module.
- [ ] Decide fixture placement (Pitfall 5): literal `concat!` strings inline (matches
      convention, zero new files) vs. a new `crates/devflow-core/tests/fixtures/`
      directory (no precedent, but publish-safe if used).
- [ ] 30c harness script (does not yet exist — `run_experiment_v3.py` is NOT reusable
      as-is, since it launches `claude` directly via Python `subprocess.Popen`, not
      through anything resembling `spawn_monitor`'s `hermetic_command`/`sh -c`
      shape).
- [ ] 30d harness script or `run_experiment_v3.py` extension (does not yet exist)
      to archive the 0.38s exit-timing measurement (currently only in harness
      stdout, per `README.md`'s own note) and the close-with-pending-tasks case.

## Security Domain

`workflow.nyquist_validation` is `true` in `.planning/config.json`; no
`security_enforcement: false` is set, so this section is required.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | no | This module has no auth boundary; it parses subprocess stdout |
| V3 Session Management | Yes (narrowly) | `session_id` extraction already has a documented mitigation (D-04/T-28-04, `agent_result.rs:246-265`): read ONLY the top-level envelope's `session_id` key, never traverse into agent-authored nested JSON (the `DEVFLOW_RESULT` marker payload), so an agent cannot redirect which session DevFlow resumes into. **30b's new session_id extraction (from the LAST `init` event) must preserve this same top-level-only discipline** — do not reuse the `json_scan` traversal helper for this field, matching the existing function's explicit design comment |
| V4 Access Control | no | N/A to this module |
| V5 Input Validation | Yes | Parsing untrusted, agent-controlled stream output is this module's entire job. `serde_json::from_str::<Value>` on untrusted input is memory-safe by construction (no unsafe, no custom parser); the real V5 risk is semantic, not memory-safety — see STRIDE row below |
| V6 Cryptography | no | N/A |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| Prompt-echo false-positive on the checkpoint gate scan (Pitfall 3) — an operator's own prompt text, once echoed back as a `user` event, is misread as the agent's OWN checkpoint declaration | Tampering (of a control-flow signal, not data) | Scope the gate-text scan to `assistant`/`result` event text only, excluding `user` events and non-top-level (`parent_tool_use_id != null`) events, per Pattern 2 + Pitfall 3's stated fix |
| An adversarial or buggy agent forges an `origin: {"kind": "task-notification"}`-shaped structure inside its OWN self-authored `DEVFLOW_RESULT` marker text (nested inside a `result.result` string, not a real top-level `result` event field) to try to influence last-result-wins selection | Spoofing | The last-result-wins logic must operate on TOP-LEVEL stream event objects only (parsed as one `Value` per JSONL line), never on nested/agent-authored JSON strings-within-strings — `origin.kind` is a field of the outer envelope object emitted by the CLI itself, structurally unreachable from inside an agent's own `result.result` text string (it would just be inert text there, same protection class as D-04's existing `session_id` mitigation) |
| Multiple `init` events reset naive "have I seen an init" state, but do NOT indicate a new/different session — mistaking this for session rotation could misattribute output across sessions in a future multi-phase parallel context | Tampering (of session attribution) | Key continuity checks off the `session_id` VALUE, never off "has an init event been seen" (see Anti-Patterns) |

## Sources

### Primary (HIGH confidence — direct source/evidence read this session)
- `crates/devflow-core/src/agent_result.rs` (2964 lines) — full function inventory
  (lines 147-1232), `parse_devflow_result`, `detect_claude_rate_limit`,
  `detect_claude_envelope_failure`, `claude_session_id`,
  `blocking_human_checkpoint_reported`/`text_reports_human_gate`,
  `is_codex_event_stream`/`parse_codex_event_result`, `parse_marker_lines`,
  `evaluate_layer1`, and the existing test module's convention (lines 1272-1824+)
- `crates/devflow-core/src/monitor.rs` (`spawn_monitor`/`spawn_monitor_inner`,
  lines 45-179) — detached `sh` script mechanics, `Stdio::null()` on all three
  streams, `hermetic_command` env-scrub integration
- `crates/devflow-core/src/git.rs:87-94` (`hermetic_command`) — env-var scrub list
  referenced (not enumerated verbatim; must be re-grepped at 30c implementation time)
- `crates/devflow-core/src/agents/claude.rs` (`ClaudeAgent::exec_command`,
  lines 15-31; `exec_resume_command`, lines 64-77) — current (unchanged this
  phase) `--output-format json` argv shape
- `crates/devflow-cli/src/pipeline_launch.rs:439,443,491` — consumers of
  `session_id_from_capture`/`checkpoint_reported_in_capture`
- `.planning/phases/30-keep-the-session-alive-past-turn-end/30a-evidence/README.md`,
  `run_experiment_v3.py`, `raw_output.jsonl` (12 lines), `raw_output_v2.jsonl` (32
  lines), `raw_output_v3.jsonl` (54 lines) — parsed and cross-checked this session
  via direct `python3 -c` inspection of every event's `type`/`subtype`/key set
- `.planning/ROADMAP.md` lines 2253+ (`### Phase 30: Keep the Session Alive Past
  Turn End (999.64)`) — authoritative scope/constraint source (no CONTEXT.md exists)
- `.planning/config.json` — `nyquist_validation: true`, no `security_enforcement`
  override, no `exa_search`/`brave_search`/`firecrawl` providers configured
- Local shell: `claude --version` (`2.1.220`), `cargo --version` (`1.97.1`),
  `cargo test -p devflow-core --lib agent_result::tests::parse_success_marker`
  (confirmed `1 passed; 0 failed`)

### Secondary (MEDIUM confidence)
None — no library/API documentation lookups were performed this session; this
phase's domain is entirely this project's own source code and archived evidence,
not a third-party library (`exa_search`/`brave_search`/`firecrawl` are all
unavailable per `init.phase-op`'s output, and none were needed).

### Tertiary (LOW confidence)
- The `task-notification` origin/delivery behavior itself (upstream Claude Code CLI
  internal behavior) — explicitly undocumented/unpinned per the review's own M2
  finding; this research does not add any external documentation for it (none
  exists that this session could locate) and treats it as `[ASSUMED]`,
  consistent with the review's own characterization.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies, reuses `serde_json`/`serde` already
  pinned; verified via `Cargo.toml` reads, not assumption.
- Architecture (the parser pattern to mirror): HIGH — `parse_codex_event_result` is
  live, tested, source-read code in this exact file; the pattern to copy is not
  speculative.
- Event catalogue (Code Examples table): HIGH for what IS present in the 3 archived
  captures (independently re-parsed this session, not trusted from the ROADMAP
  entry's prose alone); LOW for whether this catalogue is EXHAUSTIVE across all
  possible stream shapes DevFlow could ever encounter (only 3 captures exist,
  all from one harness's one prompt shape).
- Pitfalls: HIGH — Pitfalls 1, 2, 4, 5 are all directly source-verified (exact
  file:line, exact behavior traced through the code, in Pitfall 4's case
  re-executed live). Pitfall 3 is HIGH for the mechanism (verified via source +
  captures) and MEDIUM for the exact scope of the fix (a design recommendation,
  not something this research could test without 30b existing yet). Pitfall 6 is
  explicitly flagged LOW/open — honestly unresolved, not papered over.
- Security domain: MEDIUM — the V3/V5 mappings and the D-04 precedent are
  source-verified; the STRIDE threat framing is this session's own analysis
  building on the review's constraint 3, not independently cross-checked against
  a second reviewer.

**Research date:** 2026-08-02
**Valid until:** ~2026-08-16 (14 days) — SHORTER than this project's usual 30-day
default, because this research is anchored to a specific, unpinned,
`claude_code_version: 2.1.220` CLI behavior (review finding M2) that could change
upstream without notice; re-verify the event catalogue against a fresh capture if
significant time passes before 30b/30c/30d are planned or executed, or if the
locally installed `claude --version` no longer reads `2.1.220`.
