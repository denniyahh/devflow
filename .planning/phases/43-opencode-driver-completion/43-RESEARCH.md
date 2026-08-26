# Phase 43: OpenCode Driver Completion - Research

**Researched:** 2026-08-23
**Domain:** Rust CLI agent-driver adapter (headless launch + JSONL completion parsing + fail-closed health/capability probing)
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01: `build_command` becomes `opencode run "<prompt>" --auto --format json`.** Reversibility:
  costly — affects CLI spawn contract. Confirmed by `.planning/research/STACK.md` and re-verified
  live (`opencode run --help`, installed v1.18.21, STACK.md recorded 1.18.18 — presence-only doctor
  check, no version pin needed per existing pattern). `--auto` is opencode's own label for
  "auto-approve permissions not explicitly denied (dangerous!)" — same posture as Pi's
  `--no-approve` / Codex's `-a never`.
- **D-02: Prompt rendering unchanged.** `render_prompt` keeps delegating to
  `crate::prompt::render_claude_style(intent)` — legacy byte-identical text, already asserted by
  `claude_and_opencode_stay_identical_but_codex_renders_native` in `agents/mod.rs`. No reason found
  to diverge; OpenCode's DEVFLOW_RESULT marker convention is unaffected by the JSON output mode
  (the marker text still needs to be inside the model's own text response — see D-03).
- **Live-capture decision:** captured now, not deferred to research. Ran
  `opencode run "<prompt>" --auto --format json` three times in an isolated scratch directory
  (never inside this repo) with real configured credentials: a plain success case, a tool-invoking
  case, and a negative-control error case (`--model nonexistent-provider/nonexistent-model-xyz`).
  Raw output saved as evidence — see Canonical References. This directly avoids the Phase 41
  round-1 mistake (assumed Antigravity schema from `--help` alone, falsified against the live CLI).
- **D-03: OpenCode's real event schema is FLATTER than Codex's — it is NOT `turn.completed`/
  `turn.failed`/`item.completed`.** Reversibility: one-way once shipped as the parser contract.
  Verified event `type` values across all three captures: `step_start`, `text`, `tool_use`,
  `step_finish` (success path, `part.reason == "stop"`), and `error` (failure path, top-level
  `{"type":"error","error":{"name":...,"data":{"message":...}}}`, process exits non-zero). There is
  no Codex-style terminal `turn.completed`/`turn.failed` marker event and no Antigravity-style
  `event` key — do not port either shape.
- **D-04: The DEVFLOW_RESULT marker must be dug out of a `type:"text"` event's `part.text` field**
  (mirroring how Codex digs it out of `item.completed.agent_message.text` — NOT a raw top-level
  marker scan). `parse_marker_lines` runs against the extracted `part.text` string, last matching
  `text` event wins (same "last wins" convention as `parse_codex_event_result`'s marker scan).
- **D-05: An `error`-typed event is a hard failure signal**, mirroring
  `claude_stream_envelope_failure` / Codex's `turn.failed` handling: return
  `Some(AgentResult { status: Failed, reason: Some(error.data.message or error.name), ... })`. A
  marker found earlier in the stream must NOT override a later `error` event (999.107 #1's
  precedence lesson — resolve the terminal state once, apply precedence explicitly, don't let an
  earlier success marker win over a later failure).
- **D-06: Torn-JSON tail handling — copy Codex's rule verbatim.** A torn trailing line after the
  last parsed event must produce `indeterminate_capture_failure()`, not a decision based on an
  earlier marker (same rationale as `parse_codex_event_result`: the tail is exactly where a failure
  event would live).
- **Naming:** new functions should be `is_opencode_event_stream` / `parse_opencode_event_result`,
  following the `is_codex_event_stream` / `parse_codex_event_result` naming convention, wired into
  `evaluate_layer1`'s `.or_else` chain alongside the Codex/Antigravity parsers.
- **D-07: Fail-closed health check = credential check, not presence-only.** Reversibility: costly —
  changes preflight refusal behavior for every OpenCode launch. Mirrors Pi's `pi auth check --json`
  rigor. Verified live: `opencode providers list` (alias `opencode auth list`) reports actual
  configured credentials on this machine (Google/OpenAI/DeepSeek via `auth.json`, plus
  Google/DeepSeek/OpenRouter via env vars) — a real fail-closed signal exists and must be used, not
  skipped for a weaker presence-only check.
- **D-08: `opencode providers list` has NO JSON output mode** — verified live (`--help` lists no
  `--format`/`--json` flag; raw bytes confirmed ANSI color codes `\x1b[90m` + Unicode box-drawing
  glyphs, not plain text). The health check must strip ANSI escapes and parse the box-drawn provider
  list, or find another equivalently-real signal — this is NOT a simple `serde_json::from_str` like
  Pi's `auth check --json`. Left to planner/executor to design the exact parse (e.g., strip
  `\x1b\[[0-9;]*m`, then match bullet-prefixed provider name lines), but the "no credentials → error"
  and "credentials present → Ok" cases must both be unit-testable from a stubbed `opencode` binary
  (same `PathGuard`/stub pattern as `pi.rs`'s tests) since no destructive test against a live
  zero-credential machine was performed.
- **D-09: `opencode models` is explicitly REJECTED as the health signal.** Verified live: it lists
  ~423 models including opencode's own always-available `opencode/*-free` catalog entries, which
  appear to require NO user-configured credentials at all. A models-count check would false-green
  "usable" even on a machine with zero configured provider credentials, defeating OPCD-03's "fail
  closed" requirement. Do not use `models` as the readiness probe even though its output is cleaner
  (plain text, no ANSI) than `providers list`.
- **D-10: Capability discovery is IN SCOPE — probe OpenCode's subagent system.** Reversibility:
  reversible (an added, not load-bearing, `DriverCapabilities` field). OpenCode has a real
  `opencode agent list` / `opencode agent create` subsystem (verified via `--help`). Mirror Pi's
  `pi_subagent_dispatch_available` pattern: probe non-interactively, fail closed to
  `subagent_dispatch: false` on any probe failure or absence, never hard-refuse a launch over this.
  Exact probe command/heuristic (e.g. whether any non-default agent is configured) is left to
  planner/executor — no user-facing product requirement pins the exact detection rule.
- **D-11: Existing `doctor_checks()` entry's install hint is stale** —
  `cmd_check("opencode", "opencode", "--version", "cargo install opencode")` in `commands.rs`
  recommends `cargo install opencode`, but the installed binary on this machine resolves via
  Homebrew (`/home/linuxbrew/.linuxbrew/bin/opencode`), not a Rust crate — `opencode` is a JS/Bun
  CLI, not a cargo package. Noted as a pre-existing latent doc bug found during discussion, not a
  phase-43 requirement — surfaced here so planner can decide whether to fix it as a drive-by or
  leave it (Claude's discretion).
- **D-12: `every_driver_passes_the_conformance_suite` already enrolls `OpenCodeDriver`** (6-driver
  array, `agents/mod.rs`) — no registration change needed, only the trait method bodies (`health`,
  `parse_completion`, `capabilities`) need real implementations so the suite exercises real behavior
  instead of trivial defaults.

### Claude's Discretion

- Exact ANSI-stripping / provider-list parsing implementation (D-08).
- Exact subagent-detection probe/heuristic for OpenCode (D-10).
- Whether to fix the stale `cargo install opencode` doctor hint (D-11) as a drive-by in this phase
  or leave it for a docs-cleanup pass.
- Unit/integration test fixture layout (stub-binary pattern, following `pi.rs`'s `PathGuard`/
  `stub_pi_on_path` convention).

### Deferred Ideas (OUT OF SCOPE)

- **Version floor / pinning `opencode`** — not requested; presence-only doctor check stays the
  pattern (matches every other driver).
- **Fixing the stale `cargo install opencode` doctor install hint** — noted (D-11) but left to
  planner/executor discretion, not a phase-43 requirement.
- **Deeper capability probing beyond subagent dispatch** (e.g. `opencode mcp`, `opencode plugin`) —
  out of scope; only the Pi-mirrored subagent-dispatch signal was discussed.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| OPCD-01 | OpenCode driver launches headless with `--auto` + `--format json`. | `## Code Examples` §1 (`build_command`); live re-verified `opencode run --help` shows `--auto` and `--format` (`default`\|`json`) flags exist exactly as D-01 states. |
| OPCD-02 | OpenCode completion/verdict is parsed from `--format json` events (regression-tested), modeled on Codex's `parse_codex_event_result`. | `## Architecture Patterns` (event schema + parser skeleton), `## Code Examples` §2-4, three real capture files as regression fixtures (already on disk, ground truth quoted verbatim below). |
| OPCD-03 | OpenCode has a fail-closed health check + capability discovery. | `## Code Examples` §5-7 (ANSI-strip + provider-count parse, ANSI-strip verified byte-exact against a live run), `## Architecture Patterns` "Health Check" and "Subagent Capability Probe" subsections. |
</phase_requirements>

## Summary

Phase 43 replaces the Phase-37 `OpenCodeDriver` stub (`crates/devflow-core/src/agents/opencode.rs`,
28 lines, `("opencode", vec!["run".into(), prompt.to_string()])`, default trait-provided `health`/
`capabilities`/`parse_completion`) with a real driver. All three product decisions (launch argv,
event schema, health signal) are already locked in CONTEXT.md against **live** evidence — three real
`opencode run --auto --format json` captures on disk at `43-evidence/*.jsonl`, and a live re-run this
session confirming `opencode run --help`, `opencode providers list --help`, and `opencode agent
--help` still match those decisions on the currently-installed v1.18.21. This RESEARCH.md does not
re-derive any of that; it answers the three things CONTEXT.md explicitly left open: (1) the exact
Rust function signatures and file layout for the parser, modeled byte-for-byte on
`parse_codex_event_result` (`agent_result.rs:756-844`), (2) a concrete ANSI-stripping algorithm for
`opencode providers list`, verified this session against the live byte stream (`\x1b[90m` color
codes wrap the provider label; the leading box-drawing glyphs `┌ │ ● └` are plain UTF-8, not ANSI, and
survive stripping), and (3) a subagent-probe heuristic for `opencode agent list` grounded in a live
run showing the single-agent baseline shape (`build (primary)` plus a permission-rule JSON dump).

**Primary recommendation:** Add `parse_opencode_event_result` / `is_opencode_event_stream` to
`agent_result.rs` immediately after `parse_codex_event_result` (after line 849), following its
structure exactly but adapted to OpenCode's flatter, non-terminal-marked event schema (scan for a
trailing `error` event as the sole terminal-failure signal — there is no `step_finish`-typed failure
event to key off of). Rewrite `opencode.rs` to mirror `pi.rs`'s file shape: `build_command` (D-01),
`parse_completion` delegating to the new `agent_result.rs` function (matching Codex's delegation
pattern), `health` doing an ANSI-strip-then-count-lines credential check (D-07/D-08), and
`capabilities` doing a fail-closed `opencode agent list` probe (D-10). Two existing tests in
`agents/mod.rs` will break the moment `build_command`'s argv changes and must be updated in the same
commit: `drivers_reproduce_legacy_adapter_behavior` (asserts `args == ["run", "x"]`) and
`opencode_wraps_prompt_in_run` (asserts `args == ["run", prompt.as_str()]`). A third,
`default_preflight_is_ok_for_built_in_adapters`, asserts `driver_for(AgentKind::OpenCode).health(&state).is_ok()` against the trait's default no-op `health` — once OpenCode's `health` becomes a real
credential check, this assertion must be removed for OpenCode (it will spawn the real `opencode`
binary in every `cargo test` run on every machine, which is both non-hermetic and will fail on a
machine with no OpenCode credentials).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Headless CLI launch (argv construction) | Backend / Driver (`devflow-core::agents::opencode`) | — | `AgentDriver::build_command` is the sole owner of spawn argv per driver; no other tier touches it (mirrors Codex/Pi). |
| Completion/verdict parsing from JSONL events | Backend / Result-evaluation (`devflow-core::agent_result`) | Driver (`opencode.rs::parse_completion`, thin delegation) | `evaluate_layer1`'s `.or_else` cascade is the single verdict authority; drivers only wire their parser into it via `parse_completion`, exactly as Codex does. |
| Credential / provider readiness check | Backend / Driver (`opencode.rs::health`) | — | Preflight-gate concern (`devflow-cli::preflight`), but the *probe itself* is driver-owned per the `AgentDriver` trait contract — same as Pi's `health`. |
| Subagent capability discovery | Backend / Driver (`opencode.rs::capabilities`) | — | `DriverCapabilities` is a driver-declared, non-load-bearing signal consumed elsewhere (mode/stage routing); the probe lives entirely in the driver, mirroring Pi/Hermes. |
| Doctor/install-hint text | CLI / Operator-facing (`devflow-cli::commands::doctor_checks`) | — | Presentation-only; unrelated to the driver's runtime behavior (D-11 is optional). |

## Standard Stack

No new external dependencies are required for this phase. Every building block already exists in
the workspace:

| Component | Location | Purpose |
|-----------|----------|---------|
| `serde_json` | already a `devflow-core` dependency (`Cargo.toml:21`) | Parse the JSONL event lines and the DEVFLOW_RESULT marker, exactly as `parse_codex_event_result` does. |
| `tempfile` | already a `devflow-core` dev-dependency (`Cargo.toml:29`) | Build the stubbed `opencode` binary for `health`/`capabilities` unit tests, following `pi.rs`'s `stub_pi_on_path`. |
| Manual char-scan ANSI stripper | new, `opencode.rs` | No `regex` crate exists anywhere in this workspace (`Cargo.lock` has no `regex` package) `[VERIFIED: Cargo.lock — rg found zero matches for "regex" in Cargo.lock, Cargo.toml, or any crate's Cargo.toml this session]`. Adding a whole crate dependency for one ANSI-strip call site is disproportionate; a manual loop matches the project's own precedent (`strip_corruption_padding`, `agent_result.rs:462-464`, which hand-scans rather than pulling in a crate). |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Manual ANSI-strip loop | `regex` crate + `\x1b\[[0-9;]*m` pattern | Cleaner one-liner, but introduces the workspace's first `regex` dependency for a single 5-line call site; rejected per Don't-Hand-Roll-vs-Don't-Over-Import balance — this workspace's precedent (`strip_corruption_padding`) is to hand-roll simple text scrubbing. |
| `opencode providers list` credential probe | `opencode auth list` (documented alias, per D-07) | Functionally identical — same binary subcommand tree, `auth` is an alias for `providers`. Either invocation is correct; `providers list` is the canonical form used in `--help` output and is what this session's live captures are based on, so prefer it for consistency with the evidence. |
| `opencode agent list` subagent probe | `opencode agent create --help`'s flag surface alone (no live probe) | Rejected — a static flag check can't tell whether a subagent is *configured*, only that the feature exists; D-10 requires probing the actual configured agent list, matching Pi's `pi list --no-approve` live-probe pattern rather than a static capability flag. |

**Installation:** None — no new crates to add to any `Cargo.toml`.

## Package Legitimacy Audit

Not applicable — no external packages are installed by this phase. No `Cargo.toml` changes are
required; every dependency used (`serde_json`, `tempfile`) is already present in `devflow-core`.

**Packages removed due to [SLOP] verdict:** none (no packages evaluated — none proposed)
**Packages flagged as suspicious [SUS]:** none

## Architecture Patterns

### System Architecture Diagram

```
devflow start --agent opencode
        │
        ▼
 preflight (devflow-cli::preflight)
        │  calls driver.health(&state)
        ▼
 OpenCodeDriver::health()                         [D-07/D-08 — NEW this phase]
        │  spawn `opencode providers list`
        │  strip ANSI escapes from stdout
        │  count "N credentials" / "N environment variables" lines
        │
        ├─ 0 total  ──────────► Err("no OpenCode provider credential configured")
        │                              │
        │                              ▼
        │                        preflight REFUSES launch (fail-closed)
        │
        └─ >0 total ──────────► Ok(())
                                       │
                                       ▼
                          launch_stage spawns:
                          `opencode run "<prompt>" --auto --format json`   [D-01 — NEW this phase]
                                       │
                                       ▼
                          stdout captured as JSONL, one event object per line
                          (step_start / text / tool_use / step_finish / error)
                                       │
                                       ▼
                       evaluate_layer1 (agent_result.rs)
                          .or_else(parse_opencode_event_result)            [D-03..D-06 — NEW this phase]
                                       │
                     ┌─────────────────┼──────────────────────┐
                     ▼                 ▼                      ▼
              torn tail after    trailing `error`       last `text` event's
              last matching    ─►event present        ─►part.text scanned for
              event                 (D-05, D-06)         DEVFLOW_RESULT marker
                     │                 │                      │       (D-04)
                     ▼                 ▼                      ▼
         indeterminate_capture_   AgentStatus::Failed    marker found → Success/Failed
         failure() (Failed,       (reason from            per marker's own status
         copy Codex verbatim)     error.data.message      (normalise_stream_marker_
                                  or error.name)           provenance)
                                                            no marker → None
                                                            (defer to Layer 2)
        ┌──────────────────────────────────────────────────────────────────┐
        │  Capability discovery (independent of the launch/parse path):    │
        │  OpenCodeDriver::capabilities()                    [D-10 — NEW]  │
        │       spawn `opencode agent list`                                │
        │       any probe failure/absence → subagent_dispatch: false        │
        │       a non-`(primary)` agent entry present → subagent_dispatch:  │
        │       true                                                        │
        └──────────────────────────────────────────────────────────────────┘
```

### Recommended Project Structure

No new files — this phase edits two existing files:

```
crates/devflow-core/src/
├── agent_result.rs   # add is_opencode_event_stream + parse_opencode_event_result
│                     #   (new functions, ~90 lines, modeled on lines 728-849)
│                     # add .or_else(|| parse_opencode_event_result(&stdout)) to
│                     #   evaluate_layer1's chain (currently ends at line 2133)
└── agents/
    └── opencode.rs   # replace the 28-line stub: build_command (D-01),
                      #   parse_completion (delegates to agent_result.rs),
                      #   health (D-07/D-08), capabilities (D-10)
                      # + #[cfg(test)] mod tests mirroring pi.rs's stub-binary
                      #   pattern, plus fixture tests loading the three
                      #   43-evidence/*.jsonl captures directly
```

`crates/devflow-cli/src/commands.rs` is touched only if D-11's doctor-hint fix is taken as a
drive-by (single-line string literal change, lines 2317-2322).

### Pattern 1: The Codex-mirrored event-stream parser (OPCD-02)

**What:** A dedicated `parse_opencode_event_result` in `agent_result.rs`, gated by a format-detector
`is_opencode_event_stream`, wired into `evaluate_layer1`'s `.or_else` chain — the same three-part
shape every prior adapter (`Claude`/`Codex`/`Antigravity`) uses.

**When to use:** This is the ONLY correct place for OpenCode's completion logic. `parse_completion`
on the driver trait must be a thin delegation (see Codex's `codex.rs:74-76`), not a reimplementation
— `evaluate_layer1` is the single verdict cascade every result-evaluation path in the codebase
(`pipeline_launch.rs:416`) already calls.

**Format-detector gate (mirrors `is_codex_event_stream`, `agent_result.rs:728-734`):**

OpenCode's real captured events all carry a top-level `"type"` key with one of exactly five observed
string values, quoted verbatim from the live evidence files read this session
(`[VERIFIED: .planning/phases/43-opencode-driver-completion/43-evidence/opencode_success.jsonl:1-3, opencode_tool_use.jsonl:1-6, opencode_error.jsonl:1]`):

```json
{"type":"step_start", ...}
{"type":"text", ..., "part":{..., "type":"text","text":"OK", ...}}
{"type":"step_finish", ..., "part":{..., "reason":"stop", "type":"step-finish", ...}}
{"type":"tool_use", ..., "part":{"type":"tool","tool":"bash", ...}}
{"type":"error", ..., "error":{"name":"UnknownError","data":{"message":"Unexpected server error. Check server logs for details.","ref":"err_6e961b80"}}}
```

A safe, distinguishing gate (parallel to Codex's `thread.started`/`turn.*` check) is: at least one
event's top-level `"type"` is `"step_start"` or `"step_finish"` — these two are OpenCode-unique
(Codex uses `thread.started`/`turn.*`, Claude uses `system`/`init`, Antigravity uses a top-level
`event` key not `type`). Do not gate on `"error"` alone — a bare `{"type":"error",...}` object shape
is generic enough that gating solely on it risks false-positiving on an unrelated adapter's error
shape; requiring a `step_start`/`step_finish` sighting keeps the gate OpenCode-specific the same way
Codex's gate requires `thread.started`/`turn.*` rather than any single generic field.

```rust
// Source: modeled on is_codex_event_stream, agent_result.rs:728-734
pub(crate) fn is_opencode_event_stream(events: &[serde_json::Value]) -> bool {
    events.iter().any(|v| {
        v.get("type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|t| t == "step_start" || t == "step_finish")
    })
}
```

**Terminal-event resolution — the key structural difference from Codex.** Codex has an explicit
terminal-status event (`turn.completed` / `turn.failed`) to key precedence off of
(`agent_result.rs:780-785`). OpenCode's captured schema has **no equivalent terminal-status event** —
`step_finish` fires after every step (including a successful tool-use step, `opencode_tool_use.jsonl`
line 3, `"reason":"tool-calls"`), not just the run's end, and `error` is the ONLY event type that is
unambiguously terminal-and-failing (confirmed live: `opencode_error.jsonl` is a single-line capture —
the process exits non-zero the moment the error event is emitted, per D-03/D-05). The correct
adaptation of Codex's precedence rule (999.107 #1: resolve terminal state once, apply precedence
explicitly, an earlier success marker must never override a later failure) is:

1. Torn-tail check first (D-06) — identical structure to Codex, but predicate is `|_| true` (any
   event) since OpenCode has no `is_top_level`-style filtered predicate to match against; every
   emitted event matters equally.
2. Scan for ANY `type:"error"` event anywhere in the stream (not just the last line — an `error`
   terminates the run in practice, but scanning for its presence rather than assuming it's always
   last is the more defensive read of "hard failure signal" in D-05). If found: hard `Failed`,
   `reason` from `error.data.message`, falling back to `error.name` when `data.message` is absent —
   this exactly matches the shape both D-05 and the live capture show
   (`{"name":"UnknownError","data":{"message":"Unexpected server error. Check server logs for details.","ref":"err_6e961b80"}}`).
3. If no `error` event: scan `type:"text"` events in reverse for the last one whose `part.text`
   contains a `DEVFLOW_RESULT` marker (via `parse_marker_lines`) — D-04. A marker found wins.
4. No error, no marker: `None` (defer to Layer 2) — same convention as Codex's marker-less
   `turn.completed`.

```rust
// Source: modeled on parse_codex_event_result, agent_result.rs:756-849,
// adapted for OpenCode's flatter schema (D-03..D-06)
pub(crate) fn parse_opencode_event_result(stdout: &str) -> Option<AgentResult> {
    let capture = ParsedCapture::parse(stdout);
    let events = &capture.events;

    if !is_opencode_event_stream(events) {
        return None;
    }

    // D-06: same trailing-torn rule as Codex, verbatim rationale — a torn
    // trailing line is exactly where an `error` event would live.
    if capture.torn_json_after_last_matching(|_| true) {
        return Some(indeterminate_capture_failure());
    }

    // D-05: an `error` event anywhere is a hard failure signal and must not
    // be overridden by an earlier success marker (999.107 #1 precedence).
    if let Some(err_event) = events
        .iter()
        .find(|v| v.get("type").and_then(serde_json::Value::as_str) == Some("error"))
    {
        let reason = err_event
            .get("error")
            .and_then(|e| {
                e.get("data")
                    .and_then(|d| d.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .or_else(|| e.get("name").and_then(serde_json::Value::as_str))
            })
            .map(str::to_string)
            .unwrap_or_else(|| "opencode reported an error event".to_string());
        return Some(AgentResult {
            status: AgentStatus::Failed,
            exit_code: None,
            reason: Some(reason),
            commits: None,
            summary: None,
            verdict: None,
            decided_by_layer: Some(1),
        });
    }

    // D-04: the marker lives inside a `type:"text"` event's `part.text`,
    // never as a raw top-level stdout line. Last matching `text` event wins.
    let marker = events.iter().rev().find_map(|v| {
        if v.get("type").and_then(serde_json::Value::as_str) != Some("text") {
            return None;
        }
        let text = v.get("part")?.get("text")?.as_str()?;
        parse_marker_lines(text)
    });

    if let Some(result) = marker {
        return Some(normalise_stream_marker_provenance(result));
    }

    // No error, no marker: defer to Layer 2 rather than an unconditional
    // Success — a marker-less run must never silently advance a stage.
    None
}
```

**Wiring into `evaluate_layer1` (`agent_result.rs`, currently ending at line 2133):**

```rust
// Source: agent_result.rs:2125-2134 (existing chain), add one line
    let stdout = read_capture(&stdout_path(project_root, phase))?;
    detect_claude_rate_limit(&stdout)
        .map(rate_limited_result)
        .or_else(|| detect_claude_envelope_failure(&stdout))
        .or_else(|| parse_claude_event_result(&stdout))
        .or_else(|| parse_antigravity_event_result(&stdout))
        .or_else(|| parse_devflow_result(&stdout))
        .or_else(|| parse_codex_event_result(&stdout))
        .or_else(|| parse_opencode_event_result(&stdout))   // <-- NEW
        .or_else(|| detect_codex_rate_limit(&stdout).map(rate_limited_result))
```

Placement matters: put it after `parse_codex_event_result` and before the trailing Codex
rate-limit text heuristic (which stays last, "least authoritative", per the existing doc comment at
`agent_result.rs:2097-2106`). Both `parse_codex_event_result` and `parse_opencode_event_result`
independently gate on their own format detector and return `None` for anything that isn't their own
shape, so order between the two JSONL-stream parsers is safe either way — but keeping OpenCode
adjacent to Codex documents the relationship for a future reader, matching how Antigravity's parser
sits next to Claude's.

### Pattern 2: Driver-level delegation (`opencode.rs::parse_completion`)

**What:** The driver's `parse_completion` trait method is a one-line delegation to the
`agent_result.rs` function — never a reimplementation.

```rust
// Source: modeled on CodexDriver::parse_completion, codex.rs:74-76
fn parse_completion(&self, output: &str) -> Option<crate::agent_result::AgentResult> {
    crate::agent_result::parse_opencode_event_result(output)
}
```

### Pattern 3: Health check — ANSI-strip + credential-count (D-07/D-08)

**What:** `opencode providers list` prints a colorized, box-drawn summary with two sections
(`Credentials` from `auth.json`, `Environment` from env vars), each ending in a `N credentials` /
`N environment variables` count line. There is no `--format json`/`--json` flag on this subcommand
(`[VERIFIED: live 'opencode providers list --help' output this session — flag list is exactly -h/--help, -v/--version, --print-logs, --log-level, --pure; no --format or --json flag present]`).

**Live-verified raw byte shape** (captured this session, `opencode providers list 2>&1`, hex-dumped):

```
1b5b 306d 0ae2 948c 2020 4372 6564 656e  .[0m....  Creden
7469 616c 7320 1b5b 3930 6d7e 2f2e 6c6f  tials .[90m~/.lo
```

`\x1b[0m` (reset) and `\x1b[90m` (bright-black/gray) are the only ANSI SGR codes observed. After
stripping `\x1b\[[0-9;]*m` byte sequences (matched manually, no `regex` crate), the box-drawing
glyphs (`┌ │ ● └`, all plain UTF-8, not ANSI-escaped) and text survive intact
(`[VERIFIED: live 'opencode providers list 2>&1 | sed "s/\x1b\[[0-9;]*m//g"' output this session]`):

```
┌  Credentials ~/.local/share/opencode/auth.json
│
●  Google api
│
●  OpenAI oauth
│
●  DeepSeek api
│
└  3 credentials

┌  Environment
│
●  DeepSeek DEEPSEEK_API_KEY
│
●  Google GOOGLE_API_KEY
│
●  OpenRouter OPENROUTER_API_KEY
│
└  3 environment variables
```

Exit code is always 0 regardless of credential state (`[VERIFIED: opencode providers list >/dev/null 2>&1; echo $?` → `0`, this session]`) — the health check CANNOT use exit code alone and must parse
stdout content.

**Parse strategy:** strip ANSI, then sum the two count lines (`N credentials` + `N environment
variables`) via a small regex-free scan (`str::rsplit_once(' ')` on the terminal `└  N <label>`
line, parse the leading digit token). Total `0` → `Err`; total `>0` → `Ok(())`.

```rust
// Source: hand-rolled, no crate dependency (see Standard Stack above);
// strip_corruption_padding (agent_result.rs:462-464) is this workspace's
// precedent for manual text-scrubbing over pulling in `regex`.
fn strip_ansi_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Sum every "<n> credentials" / "<n> environment variable(s)" count line in
/// `opencode providers list`'s (ANSI-stripped) output. `0` means no usable
/// provider is configured — the fail-closed signal (D-07).
fn opencode_configured_provider_count(stdout: &str) -> u32 {
    strip_ansi_escapes(stdout)
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start_matches(['└', '┌', '│', '●', ' ']);
            let (num, rest) = trimmed.split_once(' ')?;
            let n: u32 = num.parse().ok()?;
            (rest.starts_with("credential") || rest.starts_with("environment variable")).then_some(n)
        })
        .sum()
}
```

**Honest limit — flag this in the plan, not silently.** No destructive test against a real
zero-credential machine was performed (D-08 states this explicitly). The exact shape of a
zero-credential `opencode providers list` run (does it omit a section entirely? print `0
credentials`? both?) is `[ASSUMED]`, not verified. The executor's unit tests must exercise a
**synthetic** zero-credential fixture (constructed by hand, following the same visual grammar
observed live) alongside the real positive-credential capture recorded above, and the plan should
call this out as an assumption needing eventual real-world confirmation (e.g. via a CI runner with no
OpenCode credentials configured, or a throwaway `HOME` override).

### Pattern 4: Subagent capability probe (D-10)

**What:** `opencode agent list` prints one block per configured agent. The baseline (zero
user-configured subagents, this machine's real state) is a single `build (primary)` entry followed by
a JSON permission-rule dump (`[VERIFIED: live 'opencode agent list' output this session, exit 0]`):

```
build (primary)
  [
  {
    "permission": "*",
    ...
```

`--mode` accepts exactly three values per `opencode agent create --help`
(`[VERIFIED: live 'opencode agent create --help' this session — --mode  agent mode [string] [choices: "all", "primary", "subagent"]]`): `"all"`, `"primary"`, `"subagent"`. The first line of each
agent's block is `<name> (<mode>)`. A dispatchable subagent is therefore any block whose header line
contains `(subagent)` or `(all)` (an `"all"`-mode agent can act as both) — the default `build` agent
is always `(primary)` and must never itself count as a dispatch target.

```rust
// Source: mirrors pi_subagent_dispatch_available (pi.rs:172-183) and Hermes's
// mockable-inner-fn pattern (hermes.rs:74-94) for spawn-free unit testing
fn opencode_subagent_dispatch_available() -> bool {
    opencode_subagent_dispatch_available_with(|| {
        std::process::Command::new("opencode")
            .args(["agent", "list"])
            .output()
    })
}

fn opencode_subagent_dispatch_available_with(
    output_fn: impl FnOnce() -> std::io::Result<std::process::Output>,
) -> bool {
    let Ok(output) = output_fn() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.contains("(subagent)") || line.contains("(all)"))
}
```

`Hermes`'s `_with(output_fn)` split (`hermes.rs:83-94`) is the preferred test-isolation pattern here
over `pi.rs`'s `PathGuard`/stub-binary — it avoids spawning a real subprocess at all in unit tests and
is simpler to reason about for a probe with no other side effects. Use `PathGuard`/stub-binary only
for `health` (Pattern 3), where the actual `Command::new("opencode")` spawn path itself is part of
what's under test (mirroring `pi.rs`'s own choice to spawn-test `health` but not
`pi_subagent_dispatch_available`... actually Pi tests BOTH via spawn — see the Validation Architecture
section below for the reconciled recommendation).

### Anti-Patterns to Avoid

- **Re-deriving the event schema from `--help` text.** This is the exact Phase-41 Antigravity
  mistake CONTEXT.md calls out by name. The schema is settled by the three real capture files on
  disk; do not "improve" or "generalize" it from documentation prose.
- **Trusting exit code for `opencode providers list`.** Verified live: exit code is always `0`
  regardless of credential state. Any health-check design that branches on exit code alone will
  false-green every run.
- **Gating the event-stream detector on `type:"error"` alone.** Too generic — could collide with a
  differently-shaped adapter's error envelope reaching the wrong parser in `evaluate_layer1`'s
  cascade. Require an OpenCode-unique event type (`step_start`/`step_finish`) in the gate.
  `Antigravity`'s parser uses a top-level `event` key (not `type`) specifically to avoid this kind of
  collision — same principle applies here.
- **Porting Codex's terminal-event-lookup structure unmodified.** Codex has an explicit
  `turn.completed`/`turn.failed` pair to resolve first, then a marker search. OpenCode has no
  terminal-status event — only `error` is decisive, and it can appear as the FIRST or ONLY event in a
  short failing run (see `opencode_error.jsonl`, a single-line capture). Scan for `error` presence,
  don't assume "last event" placement.
- **Skipping the fixture test derived from the real evidence files.** OPCD-02's own success
  criterion says "regression-tested against a real capture (not an assumed schema)" — a unit test
  that only exercises hand-constructed synthetic JSON (however schema-accurate) does not satisfy this
  without at least one test that loads and parses one of the three `43-evidence/*.jsonl` files
  verbatim.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSONL / line-shape parsing with torn-tail detection | A new bespoke JSONL scanner for OpenCode | `ParsedCapture::parse` / `torn_json_after_last_matching` (`agent_result.rs:916-994`) | Already the R1 root-cause fix for a class of prior defects (stale-success resurrection through a torn tail); every stream parser (Claude, Codex, Antigravity) shares this one implementation — OpenCode must too. |
| DEVFLOW_RESULT marker scanning | A new marker-line regex/parser | `parse_marker_lines` (`agent_result.rs:1873-1901`) | Handles the tail-budget, edge-corruption-stripping, and case-insensitive-prefix rules every other adapter already relies on; re-implementing risks silently losing one of those three hardening passes. |
| Provenance-forgery guard on a marker-derived `AgentResult` | A new "trust the layer field" check | `normalise_stream_marker_provenance` (`agent_result.rs:1849-1852`) | T-30-26: an agent's self-reported marker JSON could plant `"decided_by_layer":0` to forge Layer-0 external-probe provenance; this function is the one place that closes that hole and every stream-marker-consuming parser must call it. |
| ANSI-escape stripping | Pull in the `regex` crate | Hand-rolled char-scan (Pattern 3 above) | No `regex` dependency exists anywhere in this workspace; the codebase's own precedent (`strip_corruption_padding`) is a manual scrubber for a single-purpose text-cleanup need this size. |

**Key insight:** Every prior driver phase (37, 41, 42) that touched `agent_result.rs` reused the same
three primitives (`ParsedCapture`, `parse_marker_lines`, `normalise_stream_marker_provenance`) rather
than re-deriving stream-parsing logic per adapter. OpenCode's flatter schema changes what counts as
"terminal" (no `turn.completed`/`turn.failed` equivalent — see Pattern 1) but does not change which
shared primitives apply.

## Common Pitfalls

### Pitfall 1: Treating `step_finish` as a terminal-success marker

**What goes wrong:** A naive port of Codex's structure would look for the LAST `step_finish` event
and treat it like `turn.completed`. But `step_finish` fires after every step, including a
successful tool-use step mid-run (`opencode_tool_use.jsonl` line 3, `"reason":"tool-calls"`, followed
by a SECOND `step_start`/`text`/`step_finish` sequence). The `part.reason` field distinguishes
`"stop"` (a step that ends the turn) from `"tool-calls"` (a step that continues), but neither is a
reliable *run-level* completion signal on its own — a run could legitimately end with a
`"tool-calls"`-reason `step_finish` if OpenCode's underlying process is killed or the capture is torn
right after a tool call.
**Why it happens:** Pattern-matching Codex's shape too literally, assuming every adapter has a
single unambiguous terminal event.
**How to avoid:** Do not key parsing off `step_finish` at all (Pattern 1 above deliberately does
not). Key off `error` presence (failure) and the last `text` event's marker (success/failure per
marker) instead — matching what the three real captures actually demonstrate as decisive.
**Warning signs:** A test asserting on `part.reason == "stop"` as a completion gate will pass against
`opencode_success.jsonl` but silently mis-classify `opencode_tool_use.jsonl`'s intermediate
`"tool-calls"` reason if the logic isn't scoped correctly — verify against BOTH captures.

### Pitfall 2: Assuming `opencode providers list`'s zero-credential shape

**What goes wrong:** D-08 explicitly states no destructive test against a real zero-credential
machine was performed. A health-check implementation (or its test suite) that assumes a specific
zero-credential output shape (e.g. "the Credentials section is just omitted") without flagging it as
unverified risks shipping a parser that silently mis-classifies the one case the whole feature exists
to fail closed on.
**Why it happens:** Every live verification this session ran on a machine WITH configured
credentials — the negative case is unobservable without breaking real credentials.
**How to avoid:** Build the zero-credential test fixture by hand (a plausible but explicitly
synthetic stdout string), name it clearly as synthetic in the test, and have the plan record this as
an open assumption (see `## Assumptions Log`) rather than asserting it's verified.
**Warning signs:** A PR description or SUMMARY.md claiming "the zero-credential path is tested" when
the only evidence is a hand-written fixture, not a live run.

### Pitfall 3: Forgetting to update the two argv-asserting tests in `agents/mod.rs`

**What goes wrong:** `drivers_reproduce_legacy_adapter_behavior` (line ~235-237) and
`opencode_wraps_prompt_in_run` (line ~576-581) both hard-assert `args == ["run", "x"]` /
`args == ["run", prompt.as_str()]`. The moment `build_command` changes per D-01, both tests fail —
and since they live in `agents/mod.rs`, not `opencode.rs`, an executor scoped narrowly to
`opencode.rs` could miss them.
**Why it happens:** The conformance/regression tests for a shared trait live in the trait's own
module, not the implementor's module — an easy blind spot.
**How to avoid:** Grep `agents/mod.rs` for the literal string `"run"` before considering the argv
change complete; update both assertions to expect `["run", <prompt>, "--auto", "--format", "json"]`
(or whatever exact order the plan settles on — argv order beyond D-01's stated flags is Claude's
discretion since CONTEXT.md pins only "becomes `opencode run "<prompt>" --auto --format json`", i.e.
the four-token tail, not necessarily their internal ordering relative to each other beyond appearing
after the prompt).
**Warning signs:** `cargo test -p devflow-core --lib agents::` failing with an argv-mismatch panic
after only `opencode.rs` was touched.

### Pitfall 4: `default_preflight_is_ok_for_built_in_adapters` spawning the real `opencode` binary

**What goes wrong:** This test (`agents/mod.rs`, asserts `driver_for(AgentKind::OpenCode).health(&state).is_ok()`) currently passes because the trait's DEFAULT `health` is a no-op `Ok(())`. Once
`OpenCodeDriver::health` does a real `Command::new("opencode")` spawn (D-07), this test will (a) spawn
a real subprocess in every `cargo test` run on every machine — non-hermetic — and (b) fail outright on
any machine/CI runner without OpenCode credentials configured, which is likely most CI environments.
**Why it happens:** The test's own doc comment (`agents/mod.rs:626-629`) explicitly scopes it to "no
reviewer-set storage exists yet... none of Claude/Codex/OpenCode override it" — that premise becomes
false the moment this phase ships.
**How to avoid:** Remove `driver_for(AgentKind::OpenCode).health(&state).is_ok()` from this shared
test (Claude and Codex assertions stay — Codex's `health` is still the trait default per its own
driver file, unaffected by this phase). Add OpenCode's own `health` unit tests inside `opencode.rs`
using the `PathGuard`/stub-binary pattern, matching how Pi's real `health` behavior is tested in
`pi.rs` rather than in the shared `mod.rs` file.
**Warning signs:** `cargo test --workspace` passing locally (OpenCode is configured on the dev
machine) but failing in CI (no OpenCode credentials there) — a machine-dependent flake that is easy
to miss if only run locally.

## Code Examples

See `## Architecture Patterns` above (Patterns 1-4) for the full, source-grounded code skeletons —
they are the authoritative code examples for this phase and are not duplicated here to avoid drift
between two copies.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `opencode run "<prompt>"` (no flags, positional-only, Phase-37 stub) | `opencode run "<prompt>" --auto --format json` | This phase (43) | Enables headless auto-approval and machine-parsable JSONL completion detection; previously the driver produced human-formatted text with no reliable programmatic completion signal. |
| Trait-default `health`/`capabilities` (always `Ok(())`, always `subagent_dispatch: false`) | Real credential check (`opencode providers list`) + real subagent probe (`opencode agent list`) | This phase (43) | Preflight now genuinely refuses an OpenCode launch with no usable provider credential, matching the fail-closed posture already shipped for Pi (Phase 36/39) and Hermes (Phase 42). |

**Deprecated/outdated:** None — this is a completion of a stub, not a migration away from a prior
working implementation.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The exact stdout shape of `opencode providers list` with ZERO configured credentials (does it omit a section, print `0 credentials`, or something else) | Pattern 3 / Pitfall 2 | If the real zero-credential shape differs from the synthetic test fixture, the health check could either false-green (never refuse) or false-red (refuse even a correctly-configured machine) — both defeat OPCD-03. Needs eventual confirmation on a genuinely credential-less environment (e.g. a scratch container/CI runner with no `auth.json` and no provider env vars). |
| A2 | `opencode auth list` (D-07's cited alias) is byte-identical output to `opencode providers list` | Pattern 3 | Low risk if wrong — this phase recommends using `providers list` directly (the form this session's live evidence is based on), so even if the alias diverges slightly, the implementation doesn't depend on it. |
| A3 | `error` events can only appear as the LAST event of a stream in practice, but Pattern 1's parser deliberately does NOT assume this (scans for any `error` event, not just a trailing one) | Pattern 1 | If a future OpenCode version emits a non-terminal `error` event mid-stream (e.g. a recoverable tool error) that this phase's parser has not seen, treating ANY `error` event as an unconditional Failed could be too aggressive. The three captures on disk only show `error` as the sole and final event; a plan/executor should note this as the parser's current understanding, updatable if a future capture shows otherwise. |
| A4 | A dispatchable subagent's `opencode agent list` header line always takes the literal form `<name> (<mode>)` with mode in `{primary, subagent, all}` | Pattern 4 | This is derived from `--help`'s documented `--mode` choices plus the one live baseline (`build (primary)`) — no live capture of an actual configured subagent exists (this machine has none). If a real subagent's header line differs in punctuation/spacing, the `contains("(subagent)")` check could miss it; a substring match is somewhat resilient but not proven against real subagent output. |

## Open Questions

1. **Exact argv token order for D-01 beyond the four required tokens**
   - What we know: CONTEXT.md pins the literal string `opencode run "<prompt>" --auto --format json`
     — prompt first (positional, after `run`), then `--auto`, then `--format json`.
   - What's unclear: Whether `--format json` must be two separate argv elements (`"--format".into(), "json".into()`) or could be `"--format=json".into()` — both are valid per typical yargs-based
     CLIs (opencode is a JS/Bun CLI per D-11), and `--help`'s formatting doesn't disambiguate.
   - Recommendation: Use two separate elements (`vec!["run".into(), prompt.to_string(), "--auto".into(), "--format".into(), "json".into()]`), matching this workspace's existing convention for
     multi-token flags (see Pi's `["-p".into(), "--no-approve".into(), prompt.to_string()]` and
     Codex's `"--sandbox".into(), "workspace-write".into()` — always separate elements, never
     `key=value`).

2. **Whether `opencode auth check` (a possible single-provider-targeted analog to Pi's
   `pi auth check --json --provider <p>`) exists and would be a cleaner signal than parsing
   `providers list`**
   - What we know: `opencode providers --help` lists only `list`/`login`/`logout` — no `check`
     subcommand. `[VERIFIED: live 'opencode providers --help' this session]`
   - What's unclear: Nothing — this was checked live and confirmed absent. Not actually open;
     recorded here so a future reader doesn't re-search for it.
   - Recommendation: `providers list` (D-08's own conclusion) is confirmed the only available
     signal; no further exploration needed.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `opencode` CLI | OPCD-01/02/03 (launch, parse, health) | ✓ | 1.18.21 (Homebrew, `/home/linuxbrew/.linuxbrew/bin/opencode`) | — |
| `cargo`/`rustc` (workspace toolchain) | Build/test | ✓ | (workspace-pinned, already verified by every prior phase) | — |
| OpenCode provider credentials (Google/OpenAI/DeepSeek via `auth.json`; Google/DeepSeek/OpenRouter via env) | Live end-to-end smoke test of the health check's Ok path | ✓ (on this dev machine) | — | Unit tests use a stubbed binary regardless (see Pattern 3/4), so CI does not need real credentials. |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none — all required tooling is present.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` / `cargo test` (workspace standard — no external test framework) |
| Config file | none — `crates/devflow-core/Cargo.toml` (dev-dependencies: `tempfile = "3"`) |
| Quick run command | `cargo test -p devflow-core --lib agent_result::` and `cargo test -p devflow-core --lib agents::opencode::` |
| Full suite command | `cargo test --workspace --no-fail-fast` (per `scripts/check.sh`, line 48) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| OPCD-01 | `build_command` emits `opencode run "<prompt>" --auto --format json` | unit | `cargo test -p devflow-core --lib agents::opencode::opencode_launches_with_auto_and_json` | ❌ Wave 0 — new test in `opencode.rs`; existing `agents::mod::opencode_wraps_prompt_in_run` and `drivers_reproduce_legacy_adapter_behavior` must ALSO be updated (Pitfall 3), not left asserting the old two-token argv |
| OPCD-02 | `parse_opencode_event_result` reads the DEVFLOW_RESULT marker from a `text` event's `part.text`, resolves `error` as Failed, honors torn-tail rule | unit | `cargo test -p devflow-core --lib agent_result::opencode_` (name prefix TBD by executor, following `evaluate_layer1_parses_claude_stream_capture`-style naming at `agent_result.rs:5522`) | ❌ Wave 0 — new tests; MUST include at least one test that loads `43-evidence/opencode_success.jsonl`, `opencode_tool_use.jsonl`, and `opencode_error.jsonl` verbatim from disk (regression-tested against a real capture per OPCD-02's own success criterion — a purely synthetic fixture does not satisfy this) |
| OPCD-02 (precedence) | A marker found earlier in the stream must NOT override a later `error` event | unit (negative control) | same module, e.g. `opencode_error_event_overrides_earlier_success_marker` | ❌ Wave 0 — new test; construct a synthetic capture with a success-marker `text` event followed by an `error` event, assert `Failed` wins (mirrors the 999.107 #1 regression Codex's suite carries) |
| OPCD-02 (torn tail) | A torn trailing line after the last parsed event returns `indeterminate_capture_failure()` | unit | same module, e.g. `opencode_torn_tail_after_marker_is_indeterminate` | ❌ Wave 0 — new test; mirrors Codex's own torn-tail regression coverage |
| OPCD-03 (health, positive) | `opencode providers list` output with ≥1 credential → `Ok(())` | unit | `cargo test -p devflow-core --lib agents::opencode::preflight_accepts_configured_credentials` | ❌ Wave 0 — new test, stub-binary pattern (`PathGuard`/`stub_opencode_on_path`, mirroring `pi.rs:251-282`) |
| OPCD-03 (health, negative) | `opencode providers list` output with 0 credentials → `Err(...)` | unit (negative control) | `cargo test -p devflow-core --lib agents::opencode::preflight_rejects_zero_credentials` | ❌ Wave 0 — new test; uses the SYNTHETIC zero-credential fixture (Pitfall 2 / Assumption A1) — must be commented as synthetic, not live-verified |
| OPCD-03 (capabilities) | `opencode agent list` with only the default `build (primary)` agent → `subagent_dispatch: false`; a `(subagent)`/`(all)` line present → `true`; probe failure → `false` | unit | `cargo test -p devflow-core --lib agents::opencode::` (3 cases, mirroring `pi_capabilities_detect_subagent_dispatch` / `pi_capabilities_fail_closed_when_probe_fails`, `pi.rs:421-463`) | ❌ Wave 0 — new tests |
| OPCD-01/02/03 (conformance) | `every_driver_passes_the_conformance_suite` (`agents/mod.rs:285`) still passes with the real (non-stub) OpenCode driver | integration | `cargo test -p devflow-core --lib agents::every_driver_passes_the_conformance_suite` | ✓ — already exists, exercises the driver through `test_contract()`; no new test needed, but IS a regression gate this phase must keep green |

### Sampling Rate

- **Per task commit:** `cargo test -p devflow-core --lib agent_result:: agents::` (fast, ~seconds, scoped to the touched modules)
- **Per wave merge:** `cargo test --workspace --no-fail-fast`
- **Phase gate:** `scripts/check.sh all` (fmt + clippy `--workspace --all-targets -- -D warnings` + full workspace test suite) green before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `agent_result.rs` — new `#[cfg(test)] mod tests` cases for `is_opencode_event_stream` and
      `parse_opencode_event_result`, including the three real-capture-file regression tests
- [ ] `opencode.rs` — new `#[cfg(test)] mod tests` module (currently the file has none at all — it's
      a 28-line stub with zero test coverage today `[VERIFIED: crates/devflow-core/src/agents/opencode.rs, read in full this session — 28 lines, no #[cfg(test)] block present]`), covering `build_command`, `health` (positive/negative/probe-failure), `capabilities` (three cases)
- [ ] `agents/mod.rs` — update `drivers_reproduce_legacy_adapter_behavior` and
      `opencode_wraps_prompt_in_run` for the new argv shape; remove the OpenCode assertion from
      `default_preflight_is_ok_for_built_in_adapters` (Pitfall 3, Pitfall 4)
- [ ] No new test framework/config install needed — the workspace's existing `cargo test` harness
      covers this phase fully.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | yes (indirectly) | The driver never handles credentials itself — `opencode providers list` reads OpenCode's own `auth.json`/env vars; the driver only observes their PRESENCE (count > 0), never their VALUES. No credential material should ever appear in a log line, `reason` string, or test fixture committed to git. |
| V5 Input Validation | yes | The JSONL parser (`parse_opencode_event_result`) must treat every field access as `Option`-returning (`.get(...).and_then(...)`), matching the existing Codex/Claude parsers — a malformed or adversarially-shaped event (e.g. `error.data.message` present but not a string) must fall through to a safe default reason string, never panic. |
| V6 Cryptography | no | Not applicable — no cryptographic operation in this phase. |
| V4 Access Control | no | Not applicable — this is a local CLI subprocess adapter, no network-facing access-control surface. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| A forged `DEVFLOW_RESULT` marker inside OpenCode's own model-generated text claiming Layer-0 provenance (`"decided_by_layer":0`) | Spoofing | `normalise_stream_marker_provenance` (`agent_result.rs:1849-1852`) — already the established mitigation, MUST be called on every marker-derived `AgentResult`, exactly as the Claude/Codex/Antigravity paths do (T-30-26). |
| A truncated/torn capture at exactly the point where an `error` event would have appeared, with an earlier success marker surviving intact | Tampering (accidental, not adversarial — a killed/crashed process) | The torn-tail rule (D-06, `torn_json_after_last_matching`) — an earlier survivor must never stand in for a provably-unreadable tail; return `indeterminate_capture_failure()` instead. |
| Credential values leaking into a `health()` error `reason` string (e.g. echoing raw `opencode providers list` stdout, which could theoretically include a masked-but-partial key fragment in a future OpenCode version) | Information Disclosure | The recommended parse (Pattern 3) never echoes raw provider-list stdout into the returned `Err` string — it reports only the derived COUNT (`"no OpenCode provider credential configured"`), never the parsed line contents. Preserve this discipline: do not "helpfully" include the raw `opencode providers list` output in an error message for debugging. |

## Sources

### Primary (HIGH confidence)

- Live re-verification this session: `opencode --version` (1.18.21), `opencode run --help`,
  `opencode providers --help`, `opencode providers list --help`, `opencode providers list` (raw +
  ANSI-stripped output, exit code), `opencode agent --help`, `opencode agent list` (raw output, exit
  code), `opencode agent create --help` — all run directly against the installed CLI on this machine.
- `.planning/phases/43-opencode-driver-completion/43-evidence/opencode_success.jsonl`,
  `opencode_tool_use.jsonl`, `opencode_error.jsonl` — read in full this session, quoted verbatim above.
- `.planning/phases/43-opencode-driver-completion/43-CONTEXT.md` — all D-01 through D-12 decisions,
  read in full.
- `crates/devflow-core/src/agent_result.rs` — read the full `AgentResult`/`AgentStatus`/`Verdict`
  definitions (lines 17-161), `is_codex_event_stream`/`parse_codex_event_result` (728-849),
  `ParsedCapture`/`torn_json_after_last_matching` (895-994), `indeterminate_capture_failure`
  (1822-1835), `normalise_stream_marker_provenance` (1849-1852), `parse_marker_lines` (1873-1901),
  `evaluate_layer1` (2115-2134), `strip_corruption_padding` (462-464).
- `crates/devflow-core/src/agents/opencode.rs` — read in full (28 lines, the current stub).
- `crates/devflow-core/src/agents/codex.rs` — read in full (the direct model for the parser
  delegation pattern and argv-building conventions).
- `crates/devflow-core/src/agents/pi.rs` — read in full (the direct model for `health`'s
  credential-check pattern, `PathGuard`/`stub_pi_on_path` test infrastructure, and the
  fail-closed subagent-probe pattern).
- `crates/devflow-core/src/agents/hermes.rs` — read in full (the mockable `_with(output_fn)` test
  pattern used as an alternative to spawning a real stub binary).
- `crates/devflow-core/src/agents/mod.rs` — read in full (the `AgentDriver` trait, `driver_for`,
  every existing OpenCode-referencing test that this phase's argv change will break).
- `crates/devflow-cli/src/commands.rs` (lines 2290-2329) — the `doctor_checks()` `opencode` entry
  (D-11).
- `.planning/REQUIREMENTS.md` — OPCD-01/02/03 wording, v2.8.0 requirement set.
- `.planning/config.json` — `nyquist_validation: true`, no `security_enforcement` key (treated as
  enabled per instructions).
- `CLAUDE.md` (project root) — repo-specific constraints (worktree discipline, GSD-command
  preference, verification-habit ledger).

### Secondary (MEDIUM confidence)

- `.planning/research/STACK.md` (line 18, 44, 55-56) — cross-checked OpenCode's argv/version claim
  against the milestone-level research; matches CONTEXT.md and this session's live re-verification.

### Tertiary (LOW confidence)

- None — every claim in this document was either read directly from source this session or verified
  live against the installed CLI. Items without a live/source check are explicitly logged in
  `## Assumptions Log` above rather than stated as fact.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; the two reused workspace deps (`serde_json`,
  `tempfile`) were confirmed present in `Cargo.toml` this session.
- Architecture: HIGH — the parser design was checked line-by-line against `parse_codex_event_result`
  and against all three real capture files; the health/capability probes were checked against live
  CLI output this session.
- Pitfalls: HIGH for Pitfalls 1, 3, 4 (all directly observed in source this session — the exact
  broken tests were read and quoted). MEDIUM for Pitfall 2 (the failure mode is real and logically
  necessary, but the exact zero-credential shape it warns about is, honestly, unverified — see
  Assumption A1).

**Research date:** 2026-08-23
**Valid until:** 30 days (stable domain — an existing, shipping driver-adapter pattern in a codebase
with 5 prior precedent implementations; the only fast-moving variable is the OpenCode CLI's own event
schema, which CONTEXT.md's live captures already pin for this session's installed version 1.18.21).
