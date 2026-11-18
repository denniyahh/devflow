# Phase 53: The Question Channel - Research

**Researched:** 2026-11-18
**Domain:** MCP stdio server inside a sync Rust CLI; gate protocol extension; agent argv registration; prompt policy
**Confidence:** HIGH for the code facts (read this session, cited file:line); MEDIUM for client behaviour beyond the spike's 15 min; LOW where tagged `[ASSUMED]`

<user_constraints>
## User Constraints (from CONTEXT.md)

Provenance is per item: **(operator)** binding, **(auto)** agent default the operator may overturn, **(roadmap)** fixed by ROADMAP/REQUIREMENTS. Operator answers recorded 2026-11-18: D-01 "1 spike now" (done: `53-MCP-SPIKE.md`); D-17 "2 a".

### Locked Decisions (copied verbatim from 53-CONTEXT.md `<decisions>`)

#### The MCP-consumption spike (roadmap sequencing note)
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

#### Server shape and transport
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

#### Tool payload and validation
- **D-07 (auto):** Input: `question` (non-empty string), `options` (at least one `{id, label, description?}`,
  ids unique), `context` (string, optional), `kind` (`decision` | `blocking-human` | `discuss`), optional
  `recommended` option id. Output: `{question_id, answer: {option_id} | {text}, answered_by, answered_at}`.
- **D-08 (auto):** A malformed payload (no options, duplicate ids, a `recommended` id not offered, empty question,
  unknown kind) is refused with a typed MCP tool error before any gate opens, so nothing blocks (roadmap criterion 1
  control).

#### Operator via gate (ASK-02)
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

#### Prompts and the one-shot checkpoint (ASK-03, ASK-05)
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

#### Event log and history (ASK-04)
- **D-15 (auto):** Events: `question_asked` (question id, gate id, stage, kind, question, options, context, time)
  and `question_answered` (question id, option id or text, answering policy `operator_via_gate`, answered by,
  time). Stop or timeout records a `question_closed` with the reason instead of an answer.
- **D-16 (auto):** `devflow history` shows them in order with the rest of the run; an open question reads
  "not yet answered" (distinct from an empty text answer); a run that asked nothing shows no question rows (roadmap
  criterion 4 control). The recorded events are the Phase 54 bake-off corpus, so the full question and options are
  stored, not a summary.

#### Closing run (roadmap sequencing note)
- **D-17 (auto):** Closing the phase includes one live run on a throwaway GSD project that is not DevFlow, whose
  questions reach the operator through the channel and are recorded by ASK-04. DevFlow has no discuss stage (Phase 52
  removed Define), so the recommended shape is a `devflow start --until plan` on a project with no CONTEXT.md and a
  prompt that sends any GSD question (plan-phase's, and the discuss questions it raises) through the tool. Whether
  that yields **real GSD discuss questions**, as the roadmap asks, is open for the operator (see Notes).
  - Operator answer (2026-11-18, binding): D-17 "2 a" — the closing run is `devflow start --until plan` on a
    throwaway non-DevFlow project with no CONTEXT.md.

### Claude's Discretion
- Exact event field names, the registration file's name and location under `.devflow/`, the hidden subcommand's
  name, and the `gate show` layout.

### Deferred Ideas (OUT OF SCOPE)
- Automatic answering policies and the decision model — Phase 54.
- A DevFlow discuss stage — not in scope; raised only as the open question under D-17.
- (From Phase Boundary) the decision ledger and overrides (Phase 54), failover rotation (Phase 55).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description (REQUIREMENTS.md:64-74) | Research Support |
|----|-------------|------------------|
| ASK-01 | DevFlow serves `devflow_ask` over stdio; any supported agent calls it with question, typed options, context; the call blocks until answered and returns it | MCP minimum (§Code Examples 1), registration argv per driver (§Pattern 2), blocking/timeout knobs (§Pitfalls 1-3), payload validation (§Pattern 4) |
| ASK-02 | "operator via gate": each question becomes a gate answered by `devflow gate answer <phase> --gate-id <id> --option <id>` or free text; notify fires as for any gate | `Gates` API facts (§Pattern 3), notify block to extract from `open_and_await` (pipeline_gate.rs:772-795), every `GateKind` match site (§Pattern 5), doctor/list touch points |
| ASK-03 | Every stage prompt routes decision checkpoints, `blocking-human` gates, GSD discuss questions through the tool | prompt.rs sections to remove/replace (§Pattern 6); operator-pinned text conflict (§CONTEXT contradictions C3) |
| ASK-04 | Every question and answer on the event log with policy/operator and time; `devflow history` shows them | `events::emit`/`describe`, `history::render_timeline` (§Pattern 7) |
| ASK-05 | A `blocking-human` checkpoint under a one-shot launch is answered through the channel and the stage continues in the same session (closes 999.57, 999.127) | relaunch machinery to remove (pipeline_launch.rs:2849-2969, 1823-1855), Phase 49 fixture location (scripts/scratch-dogfood-repo.sh:1066-1090), stub-agent e2e shape (§Validation) |
</phase_requirements>

## Summary

The phase is buildable on the existing gate protocol without a new transport inside the runner. The MCP server is a
hidden `devflow` subcommand the agent spawns over stdio; it validates the payload, opens a gate through
`Gates::open_registered` (which registers this process as the waiter before publishing — gates.rs:683-725), fires the
notify hook through the same block the runner uses, waits with `Gates::await_answer` (gates.rs:859-921), claims the
answer, and returns it as a `tools/call` result. The MCP surface needed is small and stable: `initialize` (echo a
supported `protocolVersion`), `notifications/initialized`, `tools/list`, `tools/call`, `ping`, `-32601` for anything
else (which is exactly what makes Claude's `server/discover` probe fall back — spec 2026-07-28 says a legacy server
may answer it with any error), and handling of `notifications/cancelled`. Hand-roll it on `serde_json`: the official
`rmcp` 3.11.0 crate is tokio-based and the workspace has no async runtime.

The code contradicts four CONTEXT claims, and the planner must treat them as design work, not as given (§CONTEXT
contradictions): (C1) the runner's stop path does **not** close gates — `record_runner_stop` only sets `stopped`
and emits `runner_stopped`; (C2) there is one gate slot per (phase, stage) and opening a gate replaces the previous
one, so two concurrent questions in one stage would clobber each other; (C3) D-12 rewrites prompt text that an
**operator** decision pinned (2026-09-10/11, `package_verification_prohibition_is_unconditional`), which an (auto)
decision cannot overturn; (C4) nothing yet lets a `gate abort` answered to the server end the run. Two further
facts force choices: `GateKind` is a public exhaustive enum, so `GateKind::Question` (D-09) is a semver-major change
to the published `devflow-core` whatever else is done; and D-02 as written makes every launch on antigravity,
hermes, opencode and pi fail until each has a registration.

Client behaviour is settled by the spike for 15 minutes: Claude held a 900 s call with defaults; Codex cut at 300 s
unless `tool_timeout_sec` was raised, and its model polls a pending call about every 21 s (cost grows with wait).
DevFlow's default gate timeout is 3 days (config_parse.rs:15-42), above Claude's documented ≈28 h wall-clock cap
and its 30 min stdio idle abort, so the Claude registration must set the per-server `timeout` to the gate timeout.

**Primary recommendation:** Build a hand-rolled, two-thread stdio server in `devflow-cli` that owns one question gate
at a time per stage (FIFO-queue later calls), closes its own gate on answer/cancel/EOF/SIGTERM/runner-gone, and add a
runner backstop that closes any `Question` gate left at the stage when the agent exits; pass all run identity as
argv, never env.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| MCP stdio JSON-RPC loop, payload validation | CLI hidden subcommand (server process, child of the agent) | devflow-core types | Agent spawns it; it must outlive nothing but its agent |
| Question gate open / wait / claim | devflow-core `Gates` (file protocol under `.devflow/gates/`) | server process as waiter | Existing protocol; `gate` verbs already read it |
| Notify hook + `gate_fired`/`notify_fired` events | CLI shared helper (extracted from `open_and_await`) | `gates::fire_gate_notify_with_fallback` | D-11 one path; runner and server call the same helper |
| Registration (argv / JSON file) | devflow-core agent drivers (`agents/*.rs`) | CLI launch (`pipeline_launch.rs`) writes the file | Drivers own argv shape; launch owns paths and identity |
| Operator answer (`gate answer`) | CLI `commands.rs` | devflow-core answer body type | Same binding-by-gate-id rule as approve/reject |
| Abort-from-question ends the run | Runner (post-agent route) | server records the request | Only the runner may run `abort()` (pipeline_launch.rs:2925 pattern) |
| Question gate cleanup when the agent is gone | Server (own gate) | Runner backstop after agent exit | Stop path does not close gates today (C1) |
| History rendering | devflow-core `history.rs` / `events.rs` | CLI `history` verb | Read-only view over `events.jsonl` |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde_json (workspace) | 1, `preserve_order` (Cargo.toml:28) | JSON-RPC framing, payload parse, tool schema | Already the workspace JSON crate; MCP stdio is newline-delimited JSON [VERIFIED: Cargo.toml:28; CITED: modelcontextprotocol.io/specification/2025-11-25/basic/transports] |
| serde (workspace) | 1 derive | Typed request/response structs | Already used |
| clap (workspace) | 4 derive | Hidden subcommand + `gate answer` | Existing CLI; `#[command(name = "__runner", hide = true)]` precedent at main.rs:97 |
| libc (both crates) | 0.2 | SIGTERM handling if the server catches it | Already a dependency (devflow-core/Cargo.toml:23, devflow-cli/Cargo.toml:25) |

No new external package is recommended.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled JSON-RPC | `rmcp` 3.11.0 (official Rust SDK) | Its transports are tokio-based (`transport-async-rw = [tokio/io-util, tokio-util/codec]`, `cargo info rmcp@3.11.0`); pulls an async runtime into a sync workspace for ~5 methods [VERIFIED: cargo info this session] |
| Blocking `tools/call` (D-06 main path) | MCP 2025-11-25 task-augmented calls (`tasks/result`) | Spec has it (`CallToolRequestParams.task`), but neither client's support is established; not recommended [CITED: modelcontextprotocol.io/specification/2025-11-25/schema] |
| Blocking on codex | D-06 `pending` + re-call, each call held ~240 s | ~1 model turn per 240 s instead of ~1 per 21 s, but the agent must re-call faithfully; operator choice (Open Question 3) |

**Installation:** none.

## Package Legitimacy Audit

No external packages are installed by this phase. `rmcp` was looked up only to reject it.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| rmcp | crates.io | — | — | github.com/modelcontextprotocol/rust-sdk | not run (not installed) | Not used |

**Packages removed due to [SLOP] verdict:** none. **Packages flagged [SUS]:** none.

## Architecture Patterns

### System Architecture Diagram

```
operator shell                         runner (__runner, systemd user unit, KillMode=control-group)
  devflow gate list/show/answer/abort      |  writes .devflow/<registration> ; launches agent (process_group(0))
        |                                  v
        |                        agent (claude -p / codex exec)
        |                                  | spawns from registration (stdio)
        |                                  v
        |                 devflow __ask --project R --phase N --stage S --timeout-secs T
        |                   reader thread: stdin lines -> {initialize, tools/list, ping, cancelled, EOF}
        |                   worker: tools/call devflow_ask
        |                      validate payload --malformed--> isError result (no gate opened)
        |                      | ok
        |                      v
        |                   Gates::open_registered(kind=Question, context=rendered question)
        |                      -> gate_fired + notify hook (state.gate_notify_cmd) + notify_fired + question_asked
        |                      v
        +--answer (gate id)--> Gates::await_answer(cancel = cancelled | EOF | SIGTERM | runner lock free | stopped)
                               | answered            | abort                 | timeout/cancel/stop
                               v                     v                       v
                          claim, read choice    record abort request,   close gate, question_closed{reason},
                          question_answered,    isError "run aborted"   isError naming the reason
                          close gate, result         |
                               |                     v
                               v               runner, after agent exit: sees request -> abort()
                          agent continues in same session
runner after ANY agent exit: if stage gate file has gate_kind=question -> Gates::close + question_closed (backstop)
```

### Recommended Project Structure
```
crates/devflow-core/src/
├── question.rs        # QuestionSpec/QuestionOption/QuestionChoice types, payload validation, context rendering (pub, additive)
├── gates.rs           # GateKind::Question; additive helpers to write/read the question extension and choice
├── agents/*.rs        # per-driver ask registration (new defaulted trait method)
├── history.rs/events.rs  # question rows in render/describe
crates/devflow-cli/src/
├── ask_server.rs      # JSON-RPC loop, threads, gate lifecycle (new)
├── main.rs            # Command::Ask (hidden), GateCmd::Answer
├── commands.rs        # gate_answer, gate show/list rendering, doctor classification
├── pipeline_gate.rs   # extracted notify helper shared with the server
└── pipeline_launch.rs # registration write + driver call; agent-exit backstop; relaunch path removed
crates/devflow-cli/tests/ask_e2e.rs  # stub-agent e2e (new)
```

### Pattern 1: Two-thread stdio server, one request in flight
**What:** A reader thread owns stdin and parses one JSON message per line; `ping` and `initialize`/`tools/list`
are answered inline; `tools/call` is handed to a worker. Stdout writes go through one `Mutex<Stdout>` and each
message is one `serde_json::to_string` line (no pretty-print: the spec forbids embedded newlines). Logs go to stderr
only. `notifications/cancelled{requestId}` and stdin EOF set an `AtomicBool` the worker's `await_answer` `cancel`
closure reads (gates.rs:858: "`cancel` is checked before every read").
**When:** Always; a single-threaded loop cannot see `notifications/cancelled` or answer `ping` while blocked.
**Why queue later calls:** one gate slot per stage (C2). A second `tools/call` while one is open waits FIFO for the
slot (it has not yet opened a gate, so `question_asked` is emitted when its gate opens). Claude Code subagents and
parallel tool calls make concurrent calls plausible [ASSUMED].

### Pattern 2: Per-launch registration, identity in argv
Server argv (recommended; name is discretionary): `<exe> __ask --project <abs root> --phase <N> --stage <stage>
--timeout-secs <gate_timeout_secs()>`. `<exe>` = the runner's `std::env::current_exe()`, so the server is the same
build as the runner.
- **claude** — write `.devflow/ask-<padded>.mcp.json` = `{"mcpServers":{"devflow":{"type":"stdio","command":<exe>,"args":[…],"timeout":<T*1000>}}}`
  and add `--mcp-config <file>`. The per-server `timeout` (ms) raises both the wall-clock cap (`MCP_TOOL_TIMEOUT`,
  default 100000000 ms) and the 30 min stdio idle window for that server [CITED: 53-MCP-SPIKE.md "Documented knobs",
  from ctx7 docs; only `MCP_TOOL_TIMEOUT` itself was run]. The current argv already carries
  `--dangerously-skip-permissions` (claude.rs:49-57), so `--allowedTools` is not needed for permission. Do **not**
  add `--strict-mcp-config` unless the operator wants stage agents stripped of their own MCP servers (it removes them;
  the spike used it only to isolate the test). Whether `--mcp-config` without `--strict-mcp-config` merges with the
  operator's servers is [ASSUMED].
- **codex** — no file needed; insert before the prompt (the prompt is the last positional, codex.rs:66):
  `-c mcp_servers.devflow.command="<exe>"`, `-c 'mcp_servers.devflow.args=[…]'` (escape with the existing
  `escape_toml_basic_string`, codex.rs:59), `-c mcp_servers.devflow.tool_timeout_sec=<T>`,
  `-c mcp_servers.devflow.default_tools_approval_mode="approve"` [VERIFIED: 53-MCP-SPIKE.md commands block]. Do not
  disable the operator's global codex servers in production (the spike did, for isolation only). The combination with
  DevFlow's `-a never … --sandbox workspace-write` (codex.rs:35-39) was not run in the spike; the codex server log
  under `-s read-only` shows MCP servers are not confined by codex's sandbox (inference from the spike, not tested
  against `workspace-write`).
- **Driver API:** `build_command(&self, phase, prompt, extra_writable_roots)` (agents/mod.rs:82-87) cannot carry the
  registration and changing its signature is breaking; add a defaulted trait method (e.g.
  `fn ask_args(&self, reg: &AskRegistration) -> Result<Vec<String>, String>` defaulting to `Err("<driver> has no
  known MCP registration")`) and splice the args at the driver-defined position. Call sites: pipeline_launch.rs:246,
  256, 1903; preflight.rs:84 and canary.rs:296/327 (canary/preflight launches must not get a registration).

### Pattern 3: Gate lifecycle owned by the server (D-05), facts from gates.rs
- Gate file path: `format!("{padded}-{stage}.json")` under `.devflow/gates/` (gates.rs:598-600) — **one per stage**.
- `open_gate_as` "set aside" any earlier answers for the stage and overwrites the gate file (gates.rs:756-771).
- `open_registered` = `register_waiter` then publish, so the gate is never visible without its waiter (gates.rs:674-725).
- `register_waiter` holds a lock that "dies with the process" (gates.rs:533-558) — a SIGKILLed server leaves a gate
  file with `WaiterStatus::Absent`.
- `AwaitOutcome` variants: `Answered(GateAnswer)`, `Unbound { reason }`, `TimedOut`, `Cancelled` (gates.rs:572-586);
  on `Unbound` call `Gates::set_aside_unbound` and wait again with the time left — copy `await_bound_answer`
  (pipeline_launch.rs:1083-1108).
- After `Answered`: `Gates::claim` (pipeline_gate.rs:832), then `registration.finish()`, then `Gates::close` for the
  stage.
- Cancel closure inputs: the cancelled/EOF flag; `liveness::probe(&liveness::runner_lock_path(root, phase))`
  (liveness.rs:209, 345) reporting not held; `state.stopped`. Poll cost: `await_answer` backs off to reads at most 60 s
  apart (config_parse.rs:80), so a stop is seen within that bound unless the closure is also checked on a short tick
  — check the cancel closure, which runs "before every read".
- Notify: `fire_gate_notify_with_fallback(phase, stage, context, unexpected, fallback_cmd)` reads
  `DEVFLOW_GATE_NOTIFY_CMD` first (gates.rs:1778-1790), but the agent's env has that variable removed
  (`strip_notify_env`, monitor.rs:287-292), so the server must pass `state.gate_notify_cmd` as the fallback — the same
  thing the runner does (pipeline_gate.rs:783-789).

### Pattern 4: Payload validation before any gate (D-07, D-08)
Pure function in devflow-core returning a typed refusal; the server maps a refusal to a `CallToolResult` with
`isError: true` and a text naming the field. Bound sizes with the existing store bound
`GATE_CONTEXT_STORED_MAX_CHARS = 4096` (pipeline_outcomes.rs:2229) or a sibling constant; render for display with
`render_gate_context` (gates.rs:1813), which replaces control characters — agent text is untrusted.
Unknown-tool name in `tools/call` → protocol error `-32602`; tool-level failures (malformed payload, stop, timeout,
abort) → `isError: true` result, per spec ("Any errors that originate from the tool SHOULD be reported inside the
result object, with `isError` set to true") [CITED: spec 2025-11-25 schema, tools/call].

### Pattern 5: Every `GateKind` match site the new variant touches
`GateKind` is `pub`, exhaustive, "Deliberately not `#[non_exhaustive]`" (gates.rs:112-161). Variants verbatim:
`CleanPass`, `GapsFound`, `ReverifyNeeded`, `HumanNeeded`, `DisagreementOverPassed`, `DisagreementOverHumanNeeded`,
`ReaderFailure`, `AmbiguousVerdict`, `FailureCap`, `NoGapClosurePlan`, `StageFailure`, `RateLimit`, `Preflight`,
`CheckpointDeclaration`; serde `rename_all = "snake_case"` (gates.rs:118), so the new one serializes as `question`.
Exhaustive sites found (the compiler is the authoritative list):
- `pipeline_outcomes::answer_family` (pipeline_outcomes.rs:564-582) → add `AnswerFamily::Question`; that enum is
  then matched in `way_on_sentence` (884-925), `screen_answer` (966-997) and further down (status_less_step et al.).
  `screen_answer` must refuse approve/reject on a question gate (D-10), allowing only abort.
- verification-message suffix match (pipeline_outcomes.rs:1360-1367).
- test-support "all kinds" list + exhaustive `listed` match (test_support.rs:745-778).
- Non-exhaustive users that also need a decision: `commands::gate_respond_report` (commands.rs:3365-3461) via
  `screen_answer`; `answer_route_lines`; doctor `check_orphan_gate` (commands.rs:4934-4952) — a question gate is open
  with a live waiter while `gate_pending` is false, so today it reports a Problem with repair `devflow gate approve`;
  `gate_list` footer (commands.rs:3186-3190) names approve/reject/abort only.

### Pattern 6: Prompt changes (ASK-03)
- `CODE_STAGE_POLICY` (prompt.rs:114-145) holds `## Unattended decision checkpoints` (124-138), the
  package-verification prohibition (139-140), `gate_resolution_rule!()` and `checkpoint_declaration!()`.
- Plan (`idempotent_stage_prompt`, 443-464), PlanGaps (472-478), Validate (424-429), the workflow-style twins
  (608-748) and `fix_prompt_with_notes` carry no question policy today; the GapsOnly arm deliberately omits
  `CODE_STAGE_POLICY` (`code_policy_applies_to_fix_arm`, 648-653). A new shared `ask_policy!()` macro must reach
  every arm, GapsOnly included (concat! needs a macro, not a const — prompt.rs:48-52).
- `checkpoint_auto_decide_prompt` (782-795) and `GATE_RESOLUTION_RULE` (79) are `pub` in the published crate.
- Snapshots under `crates/devflow-core/src/snapshots/` (7 `full_execute_fix_prompt_*` files) change.

### Pattern 7: Events and history (ASK-04)
`events::emit(root, phase, event, fields)` is fail-soft `O_APPEND` (events.rs:15-18, 36), so the server process can
emit directly. `history::render_timeline` prints `events::describe(event)` per row (history.rs:94-118), and
`describe` shows only `kind (detail)` from keys `to|stage|status|hook|reason` (events.rs:201-213) — question text and
answer need their own rendering, and "not yet answered" needs pairing by `question_id` across rows (a
`question_asked` with no later `question_answered`/`question_closed`).

### Anti-Patterns to Avoid
- **Reading run identity or timeout from env in the server:** MCP clients build the server env themselves; whether
  arbitrary parent variables reach it is [ASSUMED] not to hold. Pass argv.
- **Opening a second gate in the same stage:** it replaces the first (gates.rs:646-650). Queue.
- **Pretty-printed JSON on stdout or any log line on stdout:** breaks framing (spec: "MUST NOT write anything to its
  stdout that is not a valid MCP message").
- **Letting a question gate outlive its agent:** `gate list` keeps showing it and doctor misreports it.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Gate files, waiter liveness, answer binding | a new answer channel | `Gates::open_registered` / `await_answer` / `claim` / `close` | 50.1's binding and crash-ordering guarantees (gates.rs:674-725, 852-921) |
| Notify | question-specific hook code | the `gate_fired`/notify/`notify_fired` block from pipeline_gate.rs:772-795, extracted | D-11 |
| Answer-slot wait with set-aside | a new loop | copy/extract `await_bound_answer` (pipeline_launch.rs:1083-1108) | R-09 semantics |
| Control-char sanitizing | ad-hoc | `render_gate_context` (gates.rs:1813) | GATE-04 |
| TOML escaping for codex `-c` | new escaper | `escape_toml_basic_string` (codex.rs) | 999.107 #2 |
| Runner liveness | pid checks | `liveness::probe` on `runner_lock_path` | lock is the authority (D-04) |

**Key insight:** the server is "one more waiter" in an existing protocol; everything risky (binding, crash order,
set-aside) already exists and is tested.

## Common Pitfalls

### Pitfall 1: Claude's caps are below DevFlow's gate timeout
**What goes wrong:** call aborted at 30 min idle (stdio default) or ≈27.8 h (`MCP_TOOL_TIMEOUT` default 100000000 ms),
while `DEVFLOW_GATE_TIMEOUT_SECS` defaults to 3 days (config_parse.rs:15-42).
**How to avoid:** per-server `"timeout": T*1000` in the registration file. **Warning sign:** `notifications/cancelled`
with `SdkError: Request timed out` in the server's stderr log (spike item 3).

### Pitfall 2: Codex default 300 s timeout and polling cost
Set `tool_timeout_sec=T`. Codex's model polls every ~21 s; the spike's 900 s hold used 1,346,920 input tokens
(1,306,880 cached). Hours-long holds cost turns throughout (Open Question 3).

### Pitfall 3: Cancellation leaves an orphan gate
Both clients send `notifications/cancelled` on timeout and move on without retry (spike item 3). The server must close
its gate and emit `question_closed{reason:"cancelled_by_client"}`; the runner backstop covers SIGKILL.

### Pitfall 4: `systemctl --user stop` SIGTERMs the server too
The unit uses `KillMode=control-group` (service.rs:649), so the server receives SIGTERM with the runner. Default action
kills it before it can emit `question_closed`; either install a handler (`stop::install_signal_pipe`, stop.rs:577,
is pub) or rely on the runner backstop — but the runner is being stopped too, so the backstop must run in
`record_runner_stop` or at next `resume` (`close_gates_for_resume`, pipeline_launch.rs:2419-2450, already closes all
stage gates). Recommend both: server handler + `resume` path.

### Pitfall 5: Filtered test false greens
Gate on one named test per command with `--no-tests=fail -E 'test(=module::path::name)'` (CLAUDE.md Repo Traps).

### Pitfall 6: `doc_check` and new surface
`Command::Ask` (hidden) and `GateCmd::Answer` are scanned (`enum_variants(&source, "Command")`/`"GateCmd"`,
doc_check.rs:363-372); a documented `.devflow/…` path needs a matching source constructor (doc_check.rs:391-399); the
help snapshot `crates/devflow-cli/tests/snapshots/devflow-help.txt` changes for `gate answer`. New `DEVFLOW_*` reads
need docs or an allowlist entry — avoid adding any in the server.

### Pitfall 7: `current_exe()` after a reinstall
`cargo install --force` mid-run replaces the binary; Linux `current_exe()` then names a deleted path [ASSUMED]. The
server spawn fails and the agent sees the tool as unavailable — D-13's fallback declaration path. Acceptable; say so
in OPERATIONS.md.

### Pitfall 8: Codex `-c mcp_servers.devflow.*` merges into an existing operator entry
If the operator's `~/.codex/config.toml` already defines `mcp_servers.devflow`, overrides merge field by field
[ASSUMED]. Override every field DevFlow relies on (command, args, env-less, timeouts, approval).

## Code Examples

### 1. JSON-RPC minimum (shape, verified against the 2025-11-25 spec and the spike)
```rust
// Source: modelcontextprotocol.io/specification/2025-11-25 (initialize, ping, tools/call); 53-MCP-SPIKE.md
const SUPPORTED: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
fn negotiate(requested: &str) -> &'static str {
    // Spec: respond with the requested version if supported, else the latest the server supports.
    SUPPORTED.iter().copied().find(|v| *v == requested).unwrap_or(SUPPORTED[0])
}
match method {
    "initialize" => reply(id, json!({
        "protocolVersion": negotiate(params["protocolVersion"].as_str().unwrap_or("")),
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "devflow", "version": env!("CARGO_PKG_VERSION") },
    })),
    "notifications/initialized" => {}                      // notification: no reply
    "ping" => reply(id, json!({})),
    "tools/list" => reply(id, json!({ "tools": [ask_tool_definition()] })),
    "tools/call" => hand_to_worker(id, params),            // reply later from the worker
    "notifications/cancelled" => cancel(params["requestId"].clone()),
    _ if id.is_some() => error(id, -32601, "method not found"), // incl. Claude's server/discover
    _ => {}                                                // unknown notification: ignore
}
// Tool failure (malformed payload, stop, timeout, abort):
json!({ "content": [{ "type": "text", "text": why }], "isError": true })
// Success:
json!({ "content": [{ "type": "text", "text": answer_text }], "structuredContent": answer_json })
```
Spike evidence: Claude 2.1.327 sent `server/discover` first, got `-32601`, fell back to `initialize` with
`2025-11-25`; codex 0.170.0 sent `2025-06-18`. Spec 2026-07-28 stdio Backward Compatibility: a client "**MUST NOT**"
key the fallback to one error code, so `-32601` is valid [CITED: modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio].

### 2. Notify block to extract (verbatim source, pipeline_gate.rs:772-795)
```rust
events::emit(project_root, state.phase, "gate_fired", serde_json::json!({
    "stage": stage.to_string(), "unexpected": unexpected, "context": event_context, "gate_kind": label.kind, }));
gates::fire_gate_notify_with_fallback(state.phase, stage, &context, unexpected, state.gate_notify_cmd.as_deref());
events::emit(project_root, state.phase, "notify_fired",
    serde_json::json!({ "stage": stage.to_string(), "unexpected": unexpected }));
```

### 3. Additive answer body (recommended over adding a field to `GateResponse`)
`GateResponse` has fields `approved`, `abort`, `note`, `responded_by` (gates.rs:163-179) and 57 struct-literal sites in
`src` (count this session); `GateAnswer` is `{ gate_id, #[serde(flatten)] response }` (gates.rs:183-190); answers
are parsed through `serde_json::from_value` with no `deny_unknown_fields` anywhere in gates.rs (classify_answer,
gates.rs:2235-2245). So a question answer can be written as the same body plus one extra key (e.g. `"choice":
{"option_id": …}` or `{"text": …}`) by a new `Gates::answer_question`, every existing reader still parses it, and the
server reads the extra key back from the slot after `Answered`. Same trick for the question spec in the gate file
(`GateFile` has 14 literal sites).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `initialize` handshake only | `server/discover` probe + per-request `_meta` ("modern" era) | spec 2026-07-28 | A legacy (initialize-era) server stays valid; clients fall back on any error |
| Agent resolves checkpoint positionally / relaunch with `claude --resume` (Phase 28/47) | Ask over MCP in the same session | this phase | Removes `relaunch_checkpoint_session` and its ceiling |

## CONTEXT contradictions and gaps (planner must resolve)

- **C1 — D-05 "the runner's existing stop path closes the gate":** false. `record_runner_stop`
  (pipeline_launch.rs:1335-1367) sets `stopped`/`stop_reason` and emits `runner_stopped`; it calls no `Gates::close`.
  Gates are closed on `resume` (`close_gates_for_resume`, pipeline_launch.rs:2419) and by specific routes
  (`close_gate` callers in pipeline_outcomes/preflight/pipeline_gate). The spike's gap (client cancel) is the same
  hole. Needs an explicit plan task (Pattern 1 cleanup + runner backstop).
- **C2 — one gate slot per (phase, stage):** gates.rs:598-600 and 646-650. D-05/D-07 assume a question can always open
  a gate; concurrent questions need queuing or refusal.
- **C3 — D-12 vs operator-pinned prompt text:** `package_verification_prohibition_is_unconditional`
  (prompt.rs:1557-1580) pins "This authority never extends to a package-verification checkpoint: do not self-resolve
  or approve one under any circumstances, and report it instead. A blocking-human gate is human-only, with one
  exception: if DevFlow resumed you specifically to resolve that gate, …" as an operator decision of 2026-09-11, and
  the doc comment says "Do not widen it without a new decision" (prompt.rs:74-78). D-12 is (auto). Routing package
  verification to the operator via the tool arguably keeps "do not self-resolve", but replacing pinned text needs the
  operator's word.
- **C4 — D-10 "`gate abort` … remains valid on a question gate" and ends the run:** the server is not the runner and
  cannot run `abort()`. Needs a mechanism, e.g. a new `State` field the server commits (State is `#[non_exhaustive]`,
  state.rs:20-34, so a field is additive) that the runner's post-agent route turns into the existing `abort()` path;
  meanwhile the server returns `isError` telling the agent to stop.
- **C5 — D-02 makes four agents unlaunchable:** antigravity, hermes, opencode and pi have no registration today, and
  "fails its launch" applies to every stage once every prompt names the tool. Either give them registrations now or
  accept that `--agent opencode|pi|hermes|antigravity` refuses until a later phase.
- **C6 — CONTEXT path:** `code_context` names `crates/devflow-core/src/agents/*.rs` (correct); the orchestrator brief
  said `crates/devflow-cli/src/agents/` (does not exist).
- **C7 — no Rust "Phase 49 checkpoint fixture":** the fixture is `scripts/scratch-dogfood-repo.sh --live-payload`
  (mode at :106, plan written at :1040-1090: first task `<task type="checkpoint:decision" gate="blocking-human">`,
  options `budget-30`, `budget-120`, `budget-300`). Criterion 3's control needs a stub-agent e2e that reproduces it
  plus, optionally, the closing live run on this fixture.
- **C8 — D-12 leaves the checkpoint machinery's other parts unnamed:** `GateKind::CheckpointDeclaration`,
  `State::checkpoint_approval`, `State::checkpoint_resumes` (state.rs:206), `mode::MAX_CHECKPOINT_RESUMES`
  (mode.rs:107), the D-07 re-scan gate (pipeline_launch.rs:2876-2939) and `verify::phase_has_blocking_human_checkpoint`
  (verify.rs:131, pub) all exist only for the relaunch. Removing the variant or pub fns is semver-major.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `--mcp-config` without `--strict-mcp-config` adds DevFlow's server and keeps the operator's | Pattern 2 | Stage agents lose their MCP servers, or DevFlow's is ignored; live run shows it |
| A2 | MCP clients do not pass arbitrary parent env vars to the server | Anti-patterns | None if argv is used anyway |
| A3 | Concurrent `devflow_ask` calls happen (parallel tool calls, subagents sharing the session's servers) | Pattern 1 | Queue is unused complexity (small) |
| A4 | GSD subagents with a restricted `tools:` list cannot see `mcp__devflow__devflow_ask`; orchestrator-level questions are the normal path | Prompts | A subagent's question is not routed; prompt must say "the orchestrator asks" |
| A5 | Codex `-a never` + `--sandbox workspace-write` + `default_tools_approval_mode="approve"` lets the call through | Pattern 2 | Codex call aborted; criterion 1 live proof catches it |
| A6 | Codex `-c mcp_servers.devflow.*` merges into an existing operator entry of that name | Pitfall 8 | Wrong command/args from operator config |
| A7 | Linux `current_exe()` names a deleted path after `cargo install --force` | Pitfall 7 | Spawn failure → D-13 fallback |
| A8 | Adding a defaulted method to the pub `AgentDriver` trait is not flagged major by cargo-semver-checks | Pattern 2 | Another semver note; the release is major anyway (Open Question 1) |
| A9 | cargo-semver-checks flags a variant added to the exhaustive pub `GateKind` and removal of pub fns as major | Open Q 1 | None for design; confirm by running the CLAUDE.md command |

## Open Questions (RESOLVED)

1. **(RESOLVED) Semver: the phase is a breaking change to `devflow-core` whatever is chosen.** `GateKind::Question` on an
   exhaustive pub enum (D-09, which also rules out a wildcard) and removing `pub fn checkpoint_auto_decide_prompt`,
   `pub const GATE_RESOLUTION_RULE`, `pub fn verify::phase_has_blocking_human_checkpoint` (D-12). The workspace is at
   `4.0.0-alpha.1` and 4.0.0 is unpublished (CLAUDE.md). Recommendation: accept it for 4.0.0, record the
   `cargo semver-checks check-release -p devflow-core --baseline-rev origin/develop --release-type minor` output in
   the phase notes, and still use the additive answer/gate-file extension (Code Example 3) to avoid ~70 literal edits.
   Operator decision.
   **Resolution (planner remit, per the orchestrator; not an operator decision):** accepted for unpublished 4.0.0;
   each plan that changes devflow-core's pub API (53-01, 53-03, 53-06, 53-08, 53-10) records the semver-checks output in its
   SUMMARY, and 53-VALIDATION.md Manual-Only M1 checks the phase-head output against the planned removals.
2. **(RESOLVED) C3: may the pinned package-verification/gate-rule text be replaced by "call `devflow_ask`"?** Recommendation:
   yes — the operator still decides; the agent still never self-resolves. Needs the operator's explicit word.
   **Resolution (operator answer 1, 2026-11-18):** YES; the report-it text stays as the fallback when the tool call
   fails, and the pin test is rewritten to the new text (53-06).
3. **(RESOLVED) Codex long holds: block (one contract) or `pending` + re-call held ~240 s?** Recommendation: block for both in
   this phase (simplest, D-06 main path, raises `tool_timeout_sec`), and measure tokens on the closing run; revisit in
   Phase 55 if codex runs unattended for hours.
   **Resolution (planner remit, per the orchestrator):** block, with `tool_timeout_sec` raised to the gate timeout
   + 60 s (53-08); codex's input tokens and hold seconds are measured on the live codex proof (53-VALIDATION.md M3).
4. **(RESOLVED) C5: what happens to `--agent opencode|pi|hermes|antigravity` launches?** Recommendation: refuse with a named
   message (D-02 as written) and log a 999.x for their registrations, since the rotation needs only claude and codex.
   Operator decision because it removes a working launch.
   **Resolution (operator answer 2, 2026-11-18):** REFUSE with a named message (D-02 as written; 53-08); their
   registrations are backlog, filed as one 999.x entry by 53-08 Task 3.
5. **(RESOLVED) C4 abort mechanism** — inside planner remit (recommend the State field route); listed so it is not lost.
   **Resolution (planner remit, per the orchestrator):** the State field route, `State::abort_requested` (53-09).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo | build/test | ✓ | 1.97.1 | — |
| cargo-nextest | named-test gates | ✓ | 0.9.152 | — |
| cargo-semver-checks | CLAUDE.md semver rule | ✓ | 0.51.0 | — |
| claude | live proof (criterion 1), closing run | ✓ | 2.1.327 | — |
| codex | live proof (criterion 1) | ✓ | codex-cli 0.170.0 | — |
| devflow (installed) | dogfood runs | ✓ | 4.0.0-alpha.1 (be5e4b8, clean) | reinstall after source changes (CLAUDE.md) |
| python3 | not needed (stub agent should be POSIX sh) | ✓ | 3.15.2 | — |

No blocking gaps.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `#[test]` via cargo-nextest 0.9.152; e2e in `crates/devflow-cli/tests/*.rs` with `tests/common` (fake agent bins: `fake_bin_dir_for`, common/mod.rs:525-545) |
| Config file | workspace Cargo.toml; `scripts/check.sh all` |
| Quick run command | `cargo nextest run -p devflow-core --lib` and `cargo nextest run -p devflow --bin devflow` |
| Full suite command | `scripts/check.sh all` (then `scripts/check-in-container.sh`) |

Gate each requirement on one named test: `cargo nextest run -p <pkg> <--lib|--bin devflow|--test ask_e2e> --no-tests=fail -E 'test(=<module::path::name>)'`. Names below are proposals.

### Phase Requirements → Test Map
| Req ID | Behavior | Type | Test (proposed) | Negative control | Exists? |
|--------|----------|------|-----------------|------------------|---------|
| ASK-01 | initialize echoes a supported version; `server/discover` gets -32601 | unit (cli) | `ask_server::tests::initialize_negotiates_a_supported_version`, `ask_server::tests::an_unknown_request_gets_method_not_found` | an unsupported requested version gets `2025-11-25`, not the echo | ❌ W0 |
| ASK-01 | malformed payloads refused before any gate opens | unit (core) | `question::tests::each_malformed_payload_is_refused_naming_its_field` (no options, dup ids, bad recommended, empty question, unknown kind) | a well-formed payload passes | ❌ W0 |
| ASK-01 | stub agent's call blocks until the gate is answered and gets the option back in the same process | e2e | `ask_e2e::a_call_blocks_until_answered_and_returns_the_option` | `ask_e2e::a_malformed_call_returns_is_error_and_opens_no_gate` (no gate file, agent exits promptly) | ❌ W0 |
| ASK-01 | driver argv carries the registration; without it the tool is absent | unit (core) | `agents::claude::tests::launch_args_name_the_ask_registration`, `agents::codex::tests::launch_args_set_tool_timeout_and_approval_before_the_prompt` | canary/preflight argv has no registration | ❌ W0 |
| ASK-01 | cancel/EOF close the gate | unit/e2e | `ask_server::tests::a_cancelled_call_closes_its_gate_and_records_question_closed`, `…stdin_eof_closes_the_gate` | an answered call records no `question_closed` | ❌ W0 |
| ASK-02 | list/show print question and options | unit (cli) | `commands::tests::gate_show_prints_a_questions_options_and_recommended_marker` | a non-question gate shows no options block | ❌ W0 |
| ASK-02 | answer binds to the named gate only | unit (cli) | `commands::tests::gate_answer_with_another_gate_id_leaves_the_question_open` | the right id resolves it | ❌ W0 |
| ASK-02 | verb separation | unit (cli) | `commands::tests::gate_approve_on_a_question_gate_is_refused_naming_gate_answer`, `…gate_answer_on_a_non_question_gate_is_refused`, `…gate_answer_refuses_an_option_not_offered` | `gate abort` on a question gate is accepted | ❌ W0 |
| ASK-02 | notify fires, `notify_fired` recorded | e2e | `ask_e2e::a_question_fires_the_notify_hook_and_records_notify_fired` (hook writes a marker file) | with notify disabled (persisted cmd empty) the marker is absent | ❌ W0 |
| ASK-03 | positional instruction gone everywhere | unit (core) | `prompt::tests::no_prompt_template_carries_positional_self_resolution` over every stage × driver render | `…_trips_on_a_planted_copy` (planted text must fail with the intended message) | ❌ W0 |
| ASK-03 | every stage prompt names `devflow_ask` for the four question kinds | unit (core) | `prompt::tests::every_stage_prompt_routes_questions_through_devflow_ask` (incl. GapsOnly, PlanGaps, fix passes) | — | ❌ W0 |
| ASK-05 | Phase-49-shaped checkpoint answered in the same session | e2e | `ask_e2e::a_blocking_human_checkpoint_continues_in_the_same_session` (stub asks with `budget-30/120/300`, test answers `budget-120`, stub writes it, commits, prints success) | assert one Code `stage_launched`, zero `checkpoint_auto_decided`; a source scan asserts `relaunch_checkpoint_session` is gone | ❌ W0 |
| ASK-05 | runner backstop | unit (cli) | `pipeline_launch::tests::an_agent_exit_closes_its_stage_question_gate` | a non-question stage gate is left alone | ❌ W0 |
| ASK-04 | events recorded with policy and time | e2e | asserted inside the ASK-01 e2e (`question_asked`, `question_answered` with `operator_via_gate`) | closed question has no `question_answered` | ❌ W0 |
| ASK-04 | history rows | unit (core) | `history::tests::an_open_question_renders_not_yet_answered`, `…an_empty_text_answer_renders_distinct_from_not_yet_answered` | `history::tests::a_run_with_no_questions_has_no_question_rows` | ❌ W0 |

Stub agent shape (POSIX sh, no python/jq): keep the server's stdin open with a subshell that prints the
`initialize`, `notifications/initialized` and `tools/call` lines, then loops `sleep` until the server's output file
contains the `"id":2` reply, piped into `devflow __ask …` with stdout to that file. The test answers the gate with
`devflow gate answer` from a second process (as runner_e2e's `gate()` helper does, runner_e2e.rs:166-180).

### Sampling Rate
- Per task commit: the task's named test(s) plus `cargo nextest run -p devflow-core --lib` or `-p devflow --bin devflow`.
- Per wave merge: `scripts/check.sh all`.
- Phase gate: full suite green, container parity, then the live proofs (claude + codex, criterion 1) and the D-17 closing run.

### Wave 0 Gaps
- [ ] `crates/devflow-cli/tests/ask_e2e.rs` + sh stub helper in `tests/common`
- [ ] `crates/devflow-core/src/question.rs` test module
- [ ] `crates/devflow-cli/src/ask_server.rs` test module

## Security Domain

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | local user only |
| V3 Session Management | no | — |
| V4 Access Control | yes | an answer binds only to the gate id it names (`GateId::parse` refuses malformed ids before any path is built, gates.rs:381-415) |
| V5 Input Validation | yes | typed payload validation (Pattern 4); size bounds; `render_gate_context` sanitizes control chars |
| V6 Cryptography | no | — |

| Pattern | STRIDE | Mitigation |
|---------|--------|-----------|
| Agent text with terminal escapes in `gate list`/notify | Tampering | `render_gate_context`; notify passes context as env, never interpolated (gates.rs:1883-1890) |
| Agent forges an answer by writing the answer slot | Spoofing/EoP | Out of scope to prevent (agent runs as the same user with `--dangerously-skip-permissions`); record `answered_by` from the CLI's `USER` (commands.rs:3388-3391) |
| Question text carrying paths into `events.jsonl` | Information disclosure | Local file; D-16 requires the full text; note it in OPERATIONS.md |
| Stuck call holding an agent forever | DoS | `--timeout-secs` from `gate_timeout_secs()`; cancel on runner-gone/stop |

## Sources

### Primary (HIGH)
- Code read this session (cited inline): `gates.rs`, `pipeline_gate.rs`, `pipeline_launch.rs`, `pipeline_outcomes.rs`,
  `commands.rs`, `main.rs`, `prompt.rs`, `agents/{mod,claude,codex}.rs`, `events.rs`, `history.rs`, `monitor.rs`,
  `stop.rs`, `state.rs`, `service.rs`, `doc_check.rs`, `doc-check-allowlist.toml`, `scripts/scratch-dogfood-repo.sh`,
  `tests/common/mod.rs`, `tests/runner_e2e.rs`.
- `53-MCP-SPIKE.md` (live, 10 launches) — client behaviour.
- ctx7 `/websites/modelcontextprotocol_io_specification_2025-11-25` — stdio transport, initialize, ping, tools/call.
- ctx7 `/websites/modelcontextprotocol_io_specification_2026-07-28` — `server/discover`, stdio backward compatibility.
- `cargo info rmcp@3.11.0` — tokio-based transports.

### Tertiary (LOW)
- Items in the Assumptions Log.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new deps; rmcp rejection verified.
- Architecture: HIGH for gate/notify/prompt facts; MEDIUM for client hold beyond 15 min.
- Pitfalls: HIGH for C1-C8 (code read); MEDIUM for A1, A5 (need the live proof).

**Research date:** 2026-11-18
**Valid until:** 2026-12-18 (client versions move faster: re-check claude/codex versions before the live proof)
