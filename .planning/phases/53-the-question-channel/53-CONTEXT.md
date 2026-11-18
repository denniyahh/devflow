# Phase 53: The Question Channel - Context

**Gathered:** 2026-11-18
**Status:** Ready for planning

Provenance is per item. **(auto)** = taken by `/gsd-discuss-phase 53 --auto` as the recommended option for a gray
area; it is an agent default, **not an operator decision**, and the operator may overturn any of them before Plan.
**(operator)** = the operator said so. **(roadmap)** = fixed by ROADMAP.md / REQUIREMENTS.md.

<domain>
## Phase Boundary

An agent running under DevFlow asks DevFlow instead of a human. DevFlow serves one MCP tool, `devflow_ask`, over
stdio; a call carries a question, typed options and context, blocks until an answer exists, and returns the answer to
the same agent session. The one policy shipped is "operator via gate": each question becomes a gate the operator
answers from the shell, with the notify hook firing as for any gate. Every stage prompt (Plan, Code, Validate) routes
decision checkpoints, `blocking-human` gates and GSD discuss questions through the tool, and every question and
answer is on `events.jsonl` and in `devflow history` (ASK-01..ASK-05; closes 999.57, 999.127; 999.175 folded via
ASK-05).

Out of scope (later phases): any automatic answering policy or decision model (Phase 54), the decision ledger and
overrides (Phase 54), failover rotation (Phase 55).

</domain>

<decisions>
## Implementation Decisions

### The MCP-consumption spike (roadmap sequencing note)
- **D-01 (auto):** The spike the roadmap places in this discuss step was **not run** by this auto discuss (it needs
  live, paid agent launches; RULE ZERO puts the costlier step to the operator). It is a **pre-planning check**, on the
  Phase 52 D-19 pattern: no plan task may implement the transport (D-03..D-06) until a recorded spike, in
  `53-MCP-SPIKE.md`, has shown for **claude** and **codex** (the two agents the rotation needs live):
  1. how a stdio MCP server is registered for a one-shot, non-interactive launch, scoped to that launch only
     (no edit to the operator's global agent config);
  2. that the agent discovers `devflow_ask` and calls it;
  3. blocking-call semantics: what the agent's MCP client does when a tool call stays open for minutes and for hours
     (client tool timeout, cancellation, retry), and the value or flag that raises it;
  4. what the agent does when the server returns a tool error (is it left blocked, does it continue).

  Control: a launch without the registration must not list the tool. If item 3 shows a call cannot stay open as
  long as an operator may take, D-06's fallback applies, and the matter returns to the operator only if it changes
  the tool's contract. Facts gathered so far (help text only, 2026-11-18, not a spike): installed `claude` has
  `--mcp-config <configs...>` and `--strict-mcp-config`; installed `codex` has an `mcp` subcommand ("Manage
  external MCP servers") and DevFlow already passes `-c key=value` overrides to `codex exec` (`agents/codex.rs`).
  Client timeouts are **unverified**.
- **D-02 (auto):** The other drivers (antigravity, hermes, opencode, pi) are covered by the driver contract's
  conformance tests and an e2e test that drives the real binary with a stub agent (roadmap criterion 1), not live.
  Each driver declares how it registers the server; a driver with no known registration fails its launch with a
  message naming what is missing (never silent), and is not given a prompt telling it to call a tool it cannot see.

### Server shape and transport
- **D-03 (auto):** The server is a hidden `devflow` subcommand (stdio), spawned by the agent from a per-launch MCP
  registration that DevFlow writes under `.devflow/` and passes on the agent's command line. The registration
  carries the project root, phase, stage and run identity, so a question is bound to the run that launched the
  agent. A hidden subcommand is still a `Command` variant: name it in the docs or `doc-check-allowlist.toml` in
  the same change.
- **D-04 (auto):** Protocol: the minimum MCP over stdio JSON-RPC — `initialize`, `tools/list`, `tools/call` for
  the one tool. Prefer a small hand-rolled serde_json implementation over an async MCP SDK, since the workspace has
  no async runtime; research confirms against the current MCP spec and may pick a sync Rust crate if one fits.
- **D-05 (auto):** The server process does the gate work itself through the existing `Gates` API (open, register
  waiter, await answer): the question becomes a gate file the operator's `gate` verbs already see, and the runner
  does not relay it. The runner's existing stop path closes the gate (Phase 52.2 D-05 semantics carry).
- **D-06 (auto):** A call blocks until answered, the run is stopped, or `DEVFLOW_GATE_TIMEOUT_SECS` elapses (no new
  timeout knob). Stop and timeout return a typed tool error naming why, so the agent is never left blocked. If the
  spike (D-01 item 3) shows the client cannot hold a call that long, the fallback is: the tool returns a typed
  `pending` result with the question id and the agent re-calls with it until answered; the launch also raises the
  client's tool timeout where the agent allows it.

### Tool payload and validation
- **D-07 (auto):** Input: `question` (non-empty string), `options` (at least one `{id, label, description?}`,
  ids unique), `context` (string, optional), `kind` (`decision` | `blocking-human` | `discuss`), optional
  `recommended` option id. Output: `{question_id, answer: {option_id} | {text}, answered_by, answered_at}`.
- **D-08 (auto):** A malformed payload (no options, duplicate ids, a `recommended` id not offered, empty question,
  unknown kind) is refused with a typed MCP tool error before any gate opens, so nothing blocks (roadmap criterion 1
  control).

### Operator via gate (ASK-02)
- **D-09 (auto):** A new `GateKind::Question` (the router has no wildcard arm, so every match site must classify
  it). `gate list` / `gate show` print the question, its options (id, label, recommended marker) and context.
- **D-10 (auto):** A new verb `devflow gate answer <phase> --gate-id <id> (--option <id> | --text <answer>)`:
  exactly one of the two; an option id the question did not offer is refused. `--gate-id` is required (one answer
  decides only the gate it names — v3.0.0 invariant, roadmap criterion 2 control). `gate approve`/`reject` on a
  question gate, and `gate answer` on a non-question gate, are refused with a message naming the right verb (one
  verb, one job). `gate abort` stays the only answer that ends a run and remains valid on a question gate. New
  `Command` variant → docs or allowlist in the same change.
- **D-11 (auto):** The notify hook fires and `notify_fired` is recorded through the same path as every other gate;
  no question-specific notify code.

### Prompts and the one-shot checkpoint (ASK-03, ASK-05)
- **D-12 (auto):** Remove the "Unattended decision checkpoints" positional self-resolution section of
  `CODE_STAGE_POLICY` and the `checkpoint_auto_decide_prompt` relaunch path (`prompt.rs`, `verify.rs`,
  `agent_result.rs`, `pipeline_launch.rs`). Every stage prompt instead says: for a decision checkpoint, a
  `blocking-human` gate, a package-verification checkpoint, or a GSD discuss question, call `devflow_ask` with the
  options and wait for the answer; do not choose positionally, do not stop with a report.
- **D-13 (auto):** The `DEVFLOW_RESULT` `checkpoint: blocking-human` declaration stays only as the fallback when
  the tool call itself fails (server unreachable), so a failure is never silent; it is no longer the normal path.
- **D-14 (auto):** Tests per roadmap criterion 3: a source-scan test that the positional instruction is gone from
  every prompt template and trips on a planted copy; the Phase 49 checkpoint fixture now ends with the stage
  continued in the same session (no relaunch, no report-and-stop).

### Event log and history (ASK-04)
- **D-15 (auto):** Events: `question_asked` (question id, gate id, stage, kind, question, options, context, time)
  and `question_answered` (question id, option id or text, answering policy `operator_via_gate`, answered by,
  time). Stop or timeout records a `question_closed` with the reason instead of an answer.
- **D-16 (auto):** `devflow history` shows them in order with the rest of the run; an open question reads
  "not yet answered" (distinct from an empty text answer); a run that asked nothing shows no question rows (roadmap
  criterion 4 control). The recorded events are the Phase 54 bake-off corpus, so the full question and options are
  stored, not a summary.

### Closing run (roadmap sequencing note)
- **D-17 (auto):** Closing the phase includes one live run on a throwaway GSD project that is not DevFlow, whose
  questions reach the operator through the channel and are recorded by ASK-04. DevFlow has no discuss stage (Phase 52
  removed Define), so the recommended shape is a `devflow start --until plan` on a project with no CONTEXT.md and a
  prompt that sends any GSD question (plan-phase's, and the discuss questions it raises) through the tool. Whether
  that yields **real GSD discuss questions**, as the roadmap asks, is open for the operator (see Notes).

### Claude's Discretion
- Exact event field names, the registration file's name and location under `.devflow/`, the hidden subcommand's
  name, and the `gate show` layout.

### Folded Todos
None (`todo.match-phase 53` returned 0 matches).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope and requirements
- `.planning/ROADMAP.md` § "Phase 53: The Question Channel" — goal, sequencing note, success criteria and controls
- `.planning/REQUIREMENTS.md` — ASK-01..ASK-05
- `.planning/ROADMAP.md` § "Phase 999.57", "Phase 999.127", "Phase 999.175" — the defects this phase closes or folds
- `.planning/ROADMAP.md` § "Phase 54: Decision Triage" — consumer of the ASK-04 corpus (shapes D-15/D-16)

### Prior decisions that carry
- `.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md` — D-13 (`gate abort`, explicit abort field), D-15..D-17
  (gate shape for rate limits), D-19 (verify-before-build pattern used by D-01), D-20 (gate answers)
- `.planning/phases/52.2-foreground-runner/52.2-CONTEXT.md` — D-01..D-07 (runner as user service, stop semantics)
- `.planning/milestones/v3.0.0-phases/50.1-split-lock-state-handling-overhaul/50.1-CONTEXT.md` — gate answer binding
  and waiter registration
- `CLAUDE.md` — doc_check rule for `Command` variants and `DEVFLOW_*` reads; dogfooding section

### MCP
- No external spec checked in. Research reads the current MCP specification (stdio transport, `tools/call`) and each
  agent's MCP docs; D-01's spike record is the authority on agent behaviour.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/devflow-core/src/gates.rs`: `Gates::open_registered`, `register_waiter`, `await_answer`, `answer`,
  `list_open`, `GateAnswer` (bound to `gate_id`), `GateKind` (no wildcard; add `Question`).
- `crates/devflow-cli/src/main.rs` `GateCmd`: `List`/`Approve`/`Reject`/`Abort`/`Show`/`Sweep` — the new
  `Answer` verb sits beside them; `commands.rs` holds `gate_respond`/`gate_show`/`gate_list`.
- `crates/devflow-core/src/events.rs` / `history.rs`: event log and history view for D-15/D-16.
- `crates/devflow-core/src/agents/{claude,codex,antigravity,hermes,opencode,pi}.rs`: `build_command` per driver —
  where the per-launch MCP registration is added (D-02, D-03).

### Established Patterns
- Gate answers are bound to a gate id and refused for any other gate; notify fires on gate open.
- Prompt policy text lives in `prompt.rs` macros (`gate_resolution_rule!`, `checkpoint_declaration!`) shared by
  several prompts — D-12 changes them in one place.
- Every `Command` variant and `DEVFLOW_*` env read needs docs or an allowlist entry (`doc_check`).

### Integration Points
- Agent launch (`pipeline_launch.rs`) — writes the registration and passes it to the driver.
- Checkpoint auto-decide relaunch (`verify::phase_has_blocking_human_checkpoint`,
  `agent_result::blocking_human_checkpoint_reported`, `prompt::checkpoint_auto_decide_prompt`) — replaced (D-12).

</code_context>

<specifics>
## Specific Ideas

- MemPalace recall (MCP, wing `devflow`): no prior decision on MCP transport; Phase 28 context recalls why the
  auto-decide relaunch exists ("no usable notification/response channel exists today") — the reason this phase
  replaces it. Hit similarity 0.47-0.54 against a nonsense-query control at 0.13.

</specifics>

<deferred>
## Deferred Ideas

- Automatic answering policies and the decision model — Phase 54.
- A DevFlow discuss stage — not in scope; raised only as the open question under D-17.

</deferred>

## Notes

- **Dogfooding (operator, 2026-11-18):** this phase IS dogfooded. Discuss ran in an agent session by convention
  (`/gsd-discuss-phase 53 --auto`; DevFlow has no discuss step). Plan runs via
  `devflow start --phase 53 --agent claude --mode supervise --until plan`; Code and Validate run via DevFlow. The
  discuss workflow's auto-advance to `/gsd-plan-phase` was therefore **not** taken, and `workflow._auto_chain_active`
  was not set.
- **Review tier: Tier B** — the orchestrator's classification, not an operator decision. Reason: the phase adds a
  tool, a gate kind, a gate verb, events and prompt text inside an unpublished `4.0.0-alpha.1` CLI; nothing in it
  deletes data, publishes, releases, merges, or changes a security boundary or the published `devflow-core` API in
  a breaking way (new `pub` items are additive; run `cargo semver-checks` per CLAUDE.md). So one external review
  round on the plan set, critical and major findings folded once.
- **Open for the operator before Plan:**
  1. D-01: the MCP-consumption spike was not run in this discuss. Run it before `devflow start --until plan`
     (matches the roadmap), or let Plan schedule it as the phase's first plan?
  2. D-17: DevFlow has no discuss stage, so how should the closing run produce "real GSD discuss questions"?
- **Operator answers (2026-11-18), recorded verbatim as operator decisions:**
  - D-01: "1 spike now" — the spike runs before `devflow start`. Result: `53-MCP-SPIKE.md`.
  - D-17: "2 a" — the closing run is `devflow start --until plan` on a throwaway non-DevFlow project with no
    CONTEXT.md.

---

*Phase: 53-the-question-channel*
*Context gathered: 2026-11-18*
