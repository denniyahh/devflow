# Phase 43: OpenCode Driver Completion - Context

**Gathered:** 2026-08-23
**Status:** Ready for planning

<domain>
## Phase Boundary

`OpenCodeDriver` (`crates/devflow-core/src/agents/opencode.rs`) currently only launches
`opencode run "<prompt>"` — a Phase 37-02 stub with no `--auto`, no JSON output, no completion
parsing, and a trivially-passing default `health()`. Phase 43 completes it: real headless launch
(`--auto --format json`), a completion/verdict parser modeled on Codex's
`parse_codex_event_result` (and regression-tested against a REAL capture, not an assumed schema),
and a fail-closed health check + capability discovery (OPCD-01, OPCD-02, OPCD-03). It must keep
passing `every_driver_passes_the_conformance_suite` (already enrolled, 6 drivers).

</domain>

<decisions>
## Implementation Decisions

### Launch & Prompt
- **D-01: `build_command` becomes `opencode run "<prompt>" --auto --format json`.** —
  **Reversibility:** costly — affects CLI spawn contract.
  Confirmed by `.planning/research/STACK.md` and re-verified live (`opencode run --help`,
  installed v1.18.21, STACK.md recorded 1.18.18 — presence-only doctor check, no version pin
  needed per existing pattern). `--auto` is opencode's own label for "auto-approve permissions not
  explicitly denied (dangerous!)" — same posture as Pi's `--no-approve` / Codex's `-a never`.
- **D-02: Prompt rendering unchanged.** `render_prompt` keeps delegating to
  `crate::prompt::render_claude_style(intent)` — legacy byte-identical text, already asserted by
  `claude_and_opencode_stay_identical_but_codex_renders_native` in `agents/mod.rs`. No reason found
  to diverge; OpenCode's DEVFLOW_RESULT marker convention is unaffected by the JSON output mode
  (the marker text still needs to be inside the model's own text response — see D-03).

### Completion Parsing (OPCD-02) — grounded in a REAL live capture
- **Live-capture decision:** captured now, not deferred to research. Ran
  `opencode run "<prompt>" --auto --format json` three times in an isolated scratch directory
  (never inside this repo) with real configured credentials: a plain success case, a tool-invoking
  case, and a negative-control error case (`--model nonexistent-provider/nonexistent-model-xyz`).
  Raw output saved as evidence — see Canonical References. This directly avoids the Phase 41
  round-1 mistake (assumed Antigravity schema from `--help` alone, falsified against the live CLI).
- **D-03: OpenCode's real event schema is FLATTER than Codex's — it is NOT `turn.completed`/
  `turn.failed`/`item.completed`.** — **Reversibility:** one-way once shipped as the parser contract.
  Verified event `type` values across all three captures: `step_start`, `text`, `tool_use`,
  `step_finish` (success path, `part.reason == "stop"`), and `error` (failure path, top-level
  `{"type":"error","error":{"name":...,"data":{"message":...}}}`, process exits non-zero).
  There is no Codex-style terminal `turn.completed`/`turn.failed` marker event and no
  Antigravity-style `event` key — do not port either shape.
- **D-04: The DEVFLOW_RESULT marker must be dug out of a `type:"text"` event's `part.text`
  field** (mirroring how Codex digs it out of `item.completed.agent_message.text` — NOT a raw
  top-level marker scan). `parse_marker_lines` runs against the extracted `part.text` string, last
  matching `text` event wins (same "last wins" convention as `parse_codex_event_result`'s marker
  scan).
- **D-05: An `error`-typed event is a hard failure signal**, mirroring
  `claude_stream_envelope_failure` / Codex's `turn.failed` handling: return
  `Some(AgentResult { status: Failed, reason: Some(error.data.message or error.name), ... })`.
  A marker found earlier in the stream must NOT override a later `error` event (999.107 #1's
  precedence lesson — resolve the terminal state once, apply precedence explicitly, don't let an
  earlier success marker win over a later failure).
- **D-06: Torn-JSON tail handling — copy Codex's rule verbatim.** A torn trailing line after the
  last parsed event must produce `indeterminate_capture_failure()`, not a decision based on an
  earlier marker (same rationale as `parse_codex_event_result`: the tail is exactly where a failure
  event would live).
- **Naming:** new functions should be `is_opencode_event_stream` / `parse_opencode_event_result`,
  following the `is_codex_event_stream` / `parse_codex_event_result` naming convention, wired into
  `evaluate_layer1`'s `.or_else` chain alongside the Codex/Antigravity parsers.

### Health Check & Capability Discovery (OPCD-03)
- **D-07: Fail-closed health check = credential check, not presence-only.** — **Reversibility:**
  costly — changes preflight refusal behavior for every OpenCode launch.
  Mirrors Pi's `pi auth check --json` rigor. Verified live: `opencode providers list` (alias
  `opencode auth list`) reports actual configured credentials on this machine (Google/OpenAI/
  DeepSeek via `auth.json`, plus Google/DeepSeek/OpenRouter via env vars) — a real fail-closed
  signal exists and must be used, not skipped for a weaker presence-only check.
- **D-08: `opencode providers list` has NO JSON output mode** — verified live (`--help` lists no
  `--format`/`--json` flag; raw bytes confirmed ANSI color codes `\x1b[90m` + Unicode box-drawing
  glyphs, not plain text). The health check must strip ANSI escapes and parse the box-drawn
  provider list, or find another equivalently-real signal — this is NOT a simple
  `serde_json::from_str` like Pi's `auth check --json`. Left to planner/executor to design the
  exact parse (e.g., strip `\x1b\[[0-9;]*m`, then match bullet-prefixed provider name lines), but
  the "no credentials → error" and "credentials present → Ok" cases must both be unit-testable
  from a stubbed `opencode` binary (same `PathGuard`/stub pattern as `pi.rs`'s tests) since no
  destructive test against a live zero-credential machine was performed.
- **D-09: `opencode models` is explicitly REJECTED as the health signal.** — Verified live: it
  lists ~423 models including opencode's own always-available `opencode/*-free` catalog entries,
  which appear to require NO user-configured credentials at all. A models-count check would false-
  green "usable" even on a machine with zero configured provider credentials, defeating OPCD-03's
  "fail closed" requirement. Do not use `models` as the readiness probe even though its output is
  cleaner (plain text, no ANSI) than `providers list`.
- **D-10: Capability discovery is IN SCOPE — probe OpenCode's subagent system.** — **Reversibility:**
  reversible (an added, not load-bearing, `DriverCapabilities` field).
  OpenCode has a real `opencode agent list` / `opencode agent create` subsystem (verified via
  `--help`). Mirror Pi's `pi_subagent_dispatch_available` pattern: probe non-interactively, fail
  closed to `subagent_dispatch: false` on any probe failure or absence, never hard-refuse a launch
  over this. Exact probe command/heuristic (e.g. whether any non-default agent is configured) is
  left to planner/executor — no user-facing product requirement pins the exact detection rule.

### Doctor / Conformance
- **D-11: Existing `doctor_checks()` entry's install hint is stale** — `cmd_check("opencode",
  "opencode", "--version", "cargo install opencode")` in `commands.rs` recommends `cargo install
  opencode`, but the installed binary on this machine resolves via Homebrew
  (`/home/linuxbrew/.linuxbrew/bin/opencode`), not a Rust crate — `opencode` is a JS/Bun CLI, not a
  cargo package. Noted as a pre-existing latent doc bug found during discussion, not a phase-43
  requirement — surfaced here so planner can decide whether to fix it as a drive-by or leave it
  (Claude's discretion, see below).
- **D-12: `every_driver_passes_the_conformance_suite` already enrolls `OpenCodeDriver`** (6-driver
  array, `agents/mod.rs`) — no registration change needed, only the trait method bodies (`health`,
  `parse_completion`, `capabilities`) need real implementations so the suite exercises real
  behavior instead of trivial defaults.

### Claude's Discretion
- Exact ANSI-stripping / provider-list parsing implementation (D-08).
- Exact subagent-detection probe/heuristic for OpenCode (D-10).
- Whether to fix the stale `cargo install opencode` doctor hint (D-11) as a drive-by in this phase
  or leave it for a docs-cleanup pass.
- Unit/integration test fixture layout (stub-binary pattern, following `pi.rs`'s `PathGuard`/
  `stub_pi_on_path` convention).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope & Requirements
- `.planning/REQUIREMENTS.md` § "OpenCode" (OPCD-01, OPCD-02, OPCD-03)
- `.planning/ROADMAP.md` § "Phase 43: OpenCode Driver Completion"
- `.planning/research/STACK.md` — OpenCode row (argv, version at time of research: 1.18.18; live
  re-verify found 1.18.21 — no functional drift found, just a version-string difference)

### Real Live-Capture Evidence (grounds D-03 through D-06)
- `.planning/phases/43-opencode-driver-completion/43-evidence/opencode_success.jsonl` — plain text
  reply, no tool use: `step_start` → `text` → `step_finish` (reason `"stop"`)
- `.planning/phases/43-opencode-driver-completion/43-evidence/opencode_tool_use.jsonl` — a
  tool-invoking turn: `step_start` → `tool_use` → `step_finish` → `step_start` → `text` →
  `step_finish`
- `.planning/phases/43-opencode-driver-completion/43-evidence/opencode_error.jsonl` — negative
  control (invalid `--model`, exit 1): single `{"type":"error",...}` event

### Prior Driver & Precedent
- `crates/devflow-core/src/agents/codex.rs` — `CodexDriver`, the direct model for D-01/D-04/D-05
  (`parse_completion` delegates to `agent_result.rs`; environment/health/interactivity patterns)
- `crates/devflow-core/src/agents/pi.rs` — `PiDriver.health()` (`pi auth check --json` +
  `classify_auth_check`), `pi_subagent_dispatch_available()`, and the `PathGuard`/`stub_pi_on_path`
  test pattern — the direct models for D-07/D-08/D-10 and the test approach
- `crates/devflow-core/src/agent_result.rs` — `parse_codex_event_result` (756-844),
  `is_claude_event_stream`, `event_is_top_level_result_marker`, `evaluate_layer1` `.or_else` chain,
  `indeterminate_capture_failure`, `claude_stream_envelope_failure`, `normalise_stream_marker_provenance`
- `crates/devflow-core/src/agents/mod.rs` — `AgentDriver` trait (`health`, `parse_completion`,
  `capabilities` defaults), `driver_for`, `every_driver_passes_the_conformance_suite` (already
  6 drivers, OpenCode enrolled), `claude_and_opencode_stay_identical_but_codex_renders_native`
- `crates/devflow-core/src/agents/opencode.rs` — current stub to replace
- `crates/devflow-cli/src/commands.rs` — `doctor_checks()`, existing `opencode` entry (stale hint,
  D-11)
- `.planning/phases/42-hermes-driver/42-CONTEXT.md` — sibling driver-completion phase, same
  structure precedent
- `.planning/phases/41-antigravity-driver/41-CONTEXT.md` — the cautionary precedent: an assumed
  schema (from `--help` only) was falsified against the live CLI across two review rounds; this
  phase's live-capture decision exists specifically to avoid repeating that failure

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `parse_marker_lines` in `crates/devflow-core/src/agent_result.rs` — reused for scanning the
  extracted `part.text` string (D-04)
- `indeterminate_capture_failure`, `claude_stream_envelope_failure`,
  `normalise_stream_marker_provenance` in `agent_result.rs` — reused verbatim for D-05/D-06
- `PathGuard` / `stub_pi_on_path` test helpers in `pi.rs` — copy the pattern for a stubbed
  `opencode` binary in tests

### Established Patterns
- Modular driver pattern (`AgentDriver` impl per agent file)
- Fail-closed capability/health probing: any probe failure or ambiguity resolves to the SAFE
  default (`subagent_dispatch: false`, health `Err`), never a hard crash and never a false-positive
  "ready"
- `evaluate_layer1`'s `.or_else` dispatch chain: each agent's event-stream parser is tried in turn,
  `None` falls through to Layer 2

### Integration Points
- `crates/devflow-core/src/agent_result.rs` (`evaluate_layer1` `.or_else` chain — add
  `parse_opencode_event_result`)
- `crates/devflow-core/src/agents/opencode.rs` (`OpenCodeDriver::build_command`, `::health`,
  `::capabilities`, `::parse_completion`)
- `crates/devflow-core/src/agents/mod.rs` (conformance suite already includes OpenCode — no wiring
  change, only richer behavior)
- `crates/devflow-cli/src/commands.rs` (`doctor_checks()` — optional D-11 fix)

</code_context>

<specifics>
## Specific Ideas

- The three real capture files under `43-evidence/` are the ground truth for OPCD-02's
  "regression-tested against a real capture (not an assumed schema)" success criterion — the
  planner/executor should turn these into the actual unit-test fixtures (or capture fresh ones
  identically, using the same isolated-scratch-directory / `--model nonexistent-provider/...`
  negative-control technique) rather than re-deriving the schema from `--help` text.

</specifics>

<deferred>
## Deferred Ideas

- **Version floor / pinning `opencode`** — not requested; presence-only doctor check stays the
  pattern (matches every other driver).
- **Fixing the stale `cargo install opencode` doctor install hint** — noted (D-11) but left to
  planner/executor discretion, not a phase-43 requirement.
- **Deeper capability probing beyond subagent dispatch** (e.g. `opencode mcp`, `opencode plugin`) —
  out of scope; only the Pi-mirrored subagent-dispatch signal was discussed.

### Reviewed Todos (not folded)
None — discussion stayed within phase scope.

</deferred>

---

*Phase: 43-OpenCode Driver Completion*
*Context gathered: 2026-08-23*
