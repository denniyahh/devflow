# Phase 53: The Question Channel - Pattern Map

**Mapped:** 2026-11-18
**Files analyzed:** 22 (5 new, 17 modified)
**Analogs found:** 21 / 22 (the stdio JSON-RPC loop itself has no analog; its threading does)

All line numbers below were read this session on `feature/phase-53` (HEAD 0dc2c7e). RESEARCH.md's cites were
spot-checked and hold, with one correction: `canary.rs` is `crates/devflow-core/src/canary.rs` (296, 327), not under
`devflow-cli`.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-core/src/question.rs` (new) | model + validator | transform | `gates.rs` `GateId::parse` 383-389, `GateError` 292-335 | role-match |
| `crates/devflow-core/src/gates.rs` (mod: `GateKind::Question`, `answer_question`, question extension) | model / file protocol | file-I/O | itself: `GateKind` 112-161, `GateResponse`/`GateAnswer` 163-190, `Gates::answer` 925-933 | exact |
| `crates/devflow-core/src/agents/mod.rs` (mod: defaulted `ask_args`) | trait | config | defaulted methods `health`/`environment` 96-103 | exact |
| `crates/devflow-core/src/agents/claude.rs` (mod) | driver | config (argv) | `build_command` 41-58 | exact |
| `crates/devflow-core/src/agents/codex.rs` (mod) | driver | config (argv) | `build_command` 26-66 (`-c` + `escape_toml_basic_string`) | exact |
| `agents/{antigravity,hermes,opencode,pi}.rs` | driver | config | inherit the trait default (refusal) | exact |
| `crates/devflow-core/src/prompt.rs` (mod) | config (prompt text) | transform | `gate_resolution_rule!` 56-62, `CODE_STAGE_POLICY` 114-145 | exact |
| `crates/devflow-core/src/events.rs` (mod `describe`) | utility | event log | `describe` 201-213 | exact |
| `crates/devflow-core/src/history.rs` (mod) | view | transform | `render_timeline` 94-117 | exact |
| `crates/devflow-core/src/verify.rs`, `agent_result.rs` (removals) | utility | — | `phase_has_blocking_human_checkpoint` 131; `blocking_human_checkpoint_reported` 716 | n/a (delete) |
| `crates/devflow-core/src/state.rs` (mod: abort-request field, C4) | model | CRUD | `State` (`#[non_exhaustive]`, state.rs:20-34) | exact |
| `crates/devflow-cli/src/ask_server.rs` (new) | service (stdio server) | request-response + streaming | threads: `stop.rs` `spawn_signal_dispatcher` 742-753; gate wait: `pipeline_gate.rs` `open_and_await` 736-850 | partial |
| `crates/devflow-cli/src/main.rs` (mod: `Command::Ask`, `GateCmd::Answer`) | route (CLI) | request-response | `Command::Runner` 92-105 + dispatch 433-441; `GateCmd::Abort` 278-295 + dispatch 479-492 | exact |
| `crates/devflow-cli/src/commands.rs` (mod: `gate_answer`, show/list, doctor) | controller | request-response | `gate_respond_report` 3365-3461; `render_gate_show` 4128; `check_orphan_gate` 4934-4952 | exact |
| `crates/devflow-cli/src/pipeline_gate.rs` (mod: extract notify helper) | service | event-driven | `open_and_await` 772-795 | exact |
| `crates/devflow-cli/src/pipeline_outcomes.rs` (mod) | router | transform | `answer_family` 564-582, `screen_answer` 966-997 | exact |
| `crates/devflow-cli/src/pipeline_launch.rs` (mod: registration, backstop, relaunch removal) | service | event-driven | `resolve_launch_shape` 237-260; `await_bound_answer` 1083-1108; `close_gates_for_resume` 2419-2450; `record_runner_stop` 1335-1367 | exact |
| `crates/devflow-cli/src/test_support.rs` (mod: all-kinds list) | test util | — | itself 745-778 | exact |
| `crates/devflow-cli/tests/ask_e2e.rs` (new) | test (e2e) | request-response | `tests/runner_e2e.rs` (`gate()` 163-178) + `tests/common/mod.rs` `fake_bin_dir_for` 531-545 | role-match |
| `crates/devflow-cli/tests/snapshots/devflow-help.txt`, `crates/devflow-core/src/snapshots/*full_execute_fix_prompt*` (7 files) | snapshot | — | regenerate | n/a |
| `OPERATIONS.md` / `ARCHITECTURE.md` / `doc-check-allowlist.toml` | docs | — | `__runner` documentation at ARCHITECTURE.md:155-156, OPERATIONS.md:52 | exact |

## Pattern Assignments

### `crates/devflow-core/src/question.rs` (new; model + validator, transform)

**Analog:** `crates/devflow-core/src/gates.rs` — `GateId::parse` (383-389) returns a typed error before any path is
built; `GateError` (292-335) is a `thiserror` enum with one `#[error("…")]` text per refusal. Copy that shape: a
`QuestionError` enum, one variant per D-08 refusal (empty question, no options, duplicate id, `recommended` not offered,
unknown kind), each message naming the field.

```rust
// gates.rs:383-389
pub fn parse(id: &str) -> Result<GateId, GateError> {
    if is_gate_id(id) { Ok(GateId(id.to_string())) } else { Err(GateError::InvalidGateId(id.to_string())) }
}
```
- Sanitize for display with `gates::render_gate_context(text, max_chars)` (gates.rs:1813); bound stored size with a
  sibling of `GATE_CONTEXT_STORED_MAX_CHARS = 4096` (pipeline_outcomes.rs:2229 — it is `pub(crate)` in the CLI, so a
  core constant is needed).
- Serde: `GateKind` uses `#[serde(rename_all = "snake_case")]` (gates.rs:118), which would give `blocking_human`;
  D-07's wire value is `blocking-human`, so the question-kind enum needs `rename_all = "kebab-case"`.
- Register with `pub mod question;` in `crates/devflow-core/src/lib.rs` beside `pub mod gates;` (lib.rs:63). Additive.

### `crates/devflow-core/src/gates.rs` (modify; file protocol)

- `GateKind` (gates.rs:112-161): "Deliberately not `#[non_exhaustive]`" — add `Question` after
  `CheckpointDeclaration` (line 160). The compiler lists every match site; known ones under Shared Patterns.
- Answer body: `GateResponse { approved, abort, note, responded_by }` (163-179), `GateAnswer { gate_id,
  #[serde(flatten)] response }` (183-190). Add a new `Gates::answer_question(...)` modelled on `Gates::answer`
  (925-933) that writes the same body plus one extra key; `classify_answer` (2235) has no `deny_unknown_fields`, so
  old readers still parse it.
- Gate file: `GateFile` (38-64) uses `#[serde(default, skip_serializing_if = "Option::is_none")]` for optional
  additions (`gate_kind` 54-55, `gsd_status` 60-61) — copy that for the question spec field.
- Lifecycle API (signatures verified): `open_registered(root, phase, stage, label, context) ->
  Result<(OpenedGate, WaiterRegistration), GateError>` (683-699); `await_answer(root, phase, stage, &gate_id,
  timeout_secs, cancel: impl Fn() -> bool) -> AwaitOutcome` (859-866); `claim(root, phase, stage, &gate_id)` (1471);
  `close(root, phase, stage) -> Result<CloseReport, GateError>` (1022-1026); `set_aside_unbound` (1305);
  `list_open` (1496). One gate file per stage: `format!("{padded}-{stage}.json")` (599) — C2.

### `crates/devflow-core/src/agents/mod.rs` + `claude.rs` + `codex.rs` (drivers; argv)

**Analog for the new trait method:** defaulted methods in `AgentDriver` (mod.rs:96-103):
```rust
fn health(&self, _state: &crate::state::State) -> Result<(), String> { Ok(()) }
fn environment(&self) -> Vec<(String, String)> { Vec::new() }
```
Add `fn ask_args(&self, reg: &AskRegistration) -> Result<Vec<String>, String>` defaulting to `Err(...)` naming the
driver; `build_command` (mod.rs:82-87) stays unchanged (signature change is breaking). Contract suite:
`test_contract` → `contract_checks` (mod.rs:115-118, 145) — add a case there so every driver is checked.

- **claude** (`claude.rs:41-58`): stream launch argv is `-p --input-format stream-json --output-format stream-json
  --verbose --dangerously-skip-permissions`; the prompt is not in argv (it goes on stdin, `StageShape::Stream`), so
  `--mcp-config <file>` can be appended. Second builder `exec_command_single_document` (claude.rs:84) and
  `exec_resume_command` (claude.rs:123) also exist; the resume one goes with the relaunch removal.
- **codex** (`codex.rs:26-66`): `-c` overrides pushed before `args.push(prompt.to_string())` (line 64), values
  escaped with `escape_toml_basic_string` and refusing non-UTF-8 paths with `expect` (49-58). Insert the
  `mcp_servers.devflow.*` overrides in the same position, same escaping.
- **Call sites to splice args at:** `pipeline_launch.rs` `resolve_launch_shape` 237-260 (lines 246, 256, and the
  Claude single-document branch 252), `pipeline_launch.rs:1903` (checkpoint resume — removed with D-12). Must NOT get
  a registration: `devflow-cli/src/preflight.rs:84`, `devflow-core/src/canary.rs:296, 327`.

### `crates/devflow-core/src/prompt.rs` (prompt policy)

**Analog:** the macro-for-`concat!` pattern (prompt.rs:48-62 doc + `gate_resolution_rule!`), reused by
`CODE_STAGE_POLICY` (114-145) through `gate_resolution_rule!()` / `checkpoint_declaration!()` (94-103). A new
`ask_policy!()` macro follows the same form and must reach every arm, including GapsOnly (which deliberately omits
`CODE_STAGE_POLICY`, `code_policy_applies_to_fix_arm` 648).
- Remove: `## Unattended decision checkpoints` block (124-140), `checkpoint_auto_decide_prompt` (782, pub),
  `GATE_RESOLUTION_RULE` (79, pub). C3: the package-verification sentence (139-140) is operator-pinned by
  `package_verification_prohibition_is_unconditional` (1572) — needs the operator's word before editing.
- `checkpoint_declaration!()` (94-103) stays, reworded as the tool-failure fallback (D-13).
- **Test analog for "every prompt × every driver":** `code_policy_reaches_the_full_execute_fix_arm_on_every_adapter`
  (prompt.rs:1083-1110) walks all six `AgentKind`s via `driver_for` with a chained `render_and_next` match; copy it
  for `every_stage_prompt_routes_questions_through_devflow_ask` and the positional-text scan (with a planted-copy
  negative control). The existing tests that assert the heading (932, 1087) will invert or go.

### `crates/devflow-core/src/events.rs` + `history.rs` (event log, view)

- Emit: `events::emit(project_root, phase, "question_asked", serde_json::json!({...}))` (events.rs:36, fail-soft
  append) — same call form as `runner_stopped` at pipeline_launch.rs:1360-1365.
- `describe` (events.rs:201-213) only prints `kind (detail)` from `to|stage|status|hook|reason`; question rows need
  their own branch.
- `render_timeline` (history.rs:94-117): one `[{timestamp}] {summary}` line per entry plus indented `  capture:` /
  `  review:` sub-lines — use indented sub-lines for question text/options/answer; "not yet answered" needs pairing by
  `question_id` across `timeline.entries` before the loop.

### `crates/devflow-cli/src/ask_server.rs` (new; stdio service)

**No analog for the JSON-RPC loop** (no stdin-reading code in either crate). Use RESEARCH.md Code Example 1.
**Thread + quit-flag analog:** `stop.rs` `SignalDispatcher` (stop.rs:710-753): `Arc<AtomicBool>` shared with a named
`std::thread::Builder` thread, `Drop` stores the flag and joins.
```rust
// stop.rs:742-753
let quit = Arc::new(AtomicBool::new(false));
let thread_quit = Arc::clone(&quit);
let thread = std::thread::Builder::new().name("runner-signals".to_string())
    .spawn(move || dispatch_signals(&token, &thread_quit))?;
```
**Gate lifecycle analog:** `pipeline_gate.rs` `open_and_await` (736-850): `open_registered` → print → `gate_fired` →
notify → `notify_fired` → wait → `screen_answer` → `Gates::claim` (832) matching `Claim::Claimed` /
`Claim::AlreadyConsumed` / `Err`, calling `registration.finish()` on every exit path.
**Wait analog:** `await_bound_answer` (pipeline_launch.rs:1083-1108) — loop with time left, `set_aside_unbound` on
`AwaitOutcome::Unbound`, `None` on `TimedOut | Cancelled`. It is a private fn in `pipeline_launch.rs`; make it
`pub(crate)` or move it so both callers share it.
**Cancel inputs:** `liveness::probe(&liveness::runner_lock_path(root, phase))` (liveness.rs:209, 345) returning
`Held::{Live, Abandoned, Missing}` (193-199); `stop::install_signal_pipe()` (stop.rs:577) is pub but "only a process
that owns a run calls it" (doc 574-576) — confirm it is fit for the server or install a local handler.
**Module:** `mod ask_server;` beside `mod pipeline_gate;` in main.rs (line 19).

### `crates/devflow-cli/src/main.rs` (CLI routes)

- Hidden subcommand: copy `Command::Runner` (main.rs:92-105): `#[command(name = "__runner", hide = true)]`,
  `#[arg(long)] project: PathBuf`, `#[arg(long)] phase: PhaseId`; dispatch 433-441 (`std::process::exit(code)` after a
  stdout flush). Add `--stage` and `--timeout-secs` the same way.
- `GateCmd::Answer`: copy `GateCmd::Abort` (278-295) fields `phase`, `--stage`, `--gate-id`, `--project`; make
  `gate_id: String` (required, D-10) and `--option`/`--text` a clap `ArgGroup` with `required = true, multiple =
  false`. Dispatch beside 479-492.

### `crates/devflow-cli/src/commands.rs` (controller)

- `gate_answer`: copy `gate_respond_report` (3365-3461): `GateId::parse` before any read (3375), `Gates::list_open`,
  `require_gate_file_names`, `refuse_a_gate_with_no_waiter`, `responded_by` from `USER` falling back to
  `"devflow-cli"` (3387-3390), `read_gate_file` + `screen_answer` refusal message listing "the answers that apply"
  (3405-3412), then write and emit `gate_response_written`.
- `gate show`: extend `render_gate_show` (4128) — pure function, unit-testable, full context via
  `render_gate_context(.., usize::MAX)`.
- `gate list` footer (3186-3190) names approve/reject/abort only; add `gate answer`.
- Doctor `check_orphan_gate` (4934-4952): today reports a live-waiter gate with `gate_pending == false` as a Problem
  with repair `devflow gate approve` — a question gate fits that shape and must be exempted.

### `crates/devflow-cli/src/pipeline_gate.rs` (extract notify helper)

Lift lines 772-795 of `open_and_await` into one `pub(crate)` helper both the runner and the server call (D-11):
```rust
events::emit(project_root, state.phase, "gate_fired", json!({ "stage": .., "unexpected": .., "context": event_context, "gate_kind": label.kind }));
gates::fire_gate_notify_with_fallback(state.phase, stage, &context, unexpected, state.gate_notify_cmd.as_deref());
events::emit(project_root, state.phase, "notify_fired", json!({ "stage": .., "unexpected": .. }));
```
The server has no `&mut State` borrow of the runner's; it loads state to get `gate_notify_cmd` (the agent env has
`DEVFLOW_GATE_NOTIFY_CMD` stripped, monitor.rs:287-292). `unexpected` comes from `state.mode.should_gate(...)`
(pipeline_gate.rs:761-765); decide what a question passes.

### `crates/devflow-cli/src/pipeline_outcomes.rs` (router)

- `answer_family` (564-582): one arm per kind, no wildcard → add `Some(GateKind::Question) => AnswerFamily::Question`
  and the variant in `AnswerFamily` (545-561).
- `screen_answer` (966-997): `GateAction::Abort(_) => Ok(())` already first (D-10 abort stays valid); add
  `AnswerFamily::Question` to refuse Advance and LoopBack with a `Refusal` naming `gate answer`. Also update
  `way_on_sentence` (884) and the verification-message suffix match (~1360).

### `crates/devflow-cli/src/pipeline_launch.rs` (launch, backstop, removals)

- Registration write + argv: in `resolve_launch_shape` (237-260); `<exe>` from `std::env::current_exe()` (no current
  use in this file).
- Agent-exit backstop: pattern from `close_gates_for_resume` (2419-2450) — read gate ids before the close, then
  `Gates::close_phase`, warn and continue on error. The backstop closes only a stage gate whose `gate_kind ==
  Some(Question)`.
- Stop path: `record_runner_stop` (1335-1367) closes no gates (C1); if a close is added, follow its "logged, never an
  error" style (`warn!` and return).
- Remove: `relaunch_checkpoint_session` (1823-1855), the D-07 re-scan route (2849-2941, calls at 2853, 2911, 2941),
  resume shape resolver (~262-280), tests at 5403-5430.

### `crates/devflow-cli/tests/ask_e2e.rs` (new e2e)

**Analog:** `tests/runner_e2e.rs` — header `mod common; use common::HermeticPath;` (9-11), devflow children through
`common::hermetic_devflow` (common/mod.rs:396), operator answers from a second process via the `gate()` helper
(runner_e2e.rs:163-178, asserts exit 0 with `shown(&output)`), event reads via `events_of(kind)`, waits via
`common::wait_for(predicate, timeout, what)` (common/mod.rs:631), child reaping via `kill_and_reap` (643). Stub agent:
`common::fake_bin_dir_for("claude", body)` (common/mod.rs:531-545) writes `#!/bin/sh\n{body}` with mode 0755 plus
`git`/`sh` links.

## Shared Patterns

### Every `GateKind` match site (compiler is authoritative)
`pipeline_outcomes.rs:564-582` (`answer_family`), `test_support.rs:745-778` (all-kinds array + exhaustive `listed`
match — add `GateKind::Question` to both), plus `AnswerFamily` matches in `screen_answer` 966-997 and `way_on_sentence`
884. Run `cargo build` after adding the variant and fix each error; no wildcard arms.

### Gate answer binding (V4)
`GateId::parse(...)` before any path (commands.rs:3375; gates.rs:383); one answer decides only the gate id it names.

### Untrusted text
`render_gate_context(text, max)` (gates.rs:1813) for every agent string shown in `gate list`/`show`/notify/history.

### Logging
Server: stderr only (stdout is MCP framing). Runner-side helpers: `info!`/`warn!` + `eprintln!("phase {phase}: …")`
as in `record_runner_stop` (pipeline_launch.rs:1340-1366).

### doc_check
`doc_check.rs:363-372` checks documented `devflow <x>` against `Command`/`GateCmd` variants; `:416-424` requires every
`Command` variant in scoped docs as `devflow <kebab-name>` (lowercased) or an allowlist entry
(`[[exceptions]] kind / token / reason`, doc-check-allowlist.toml:3-6). `Runner` is satisfied by ARCHITECTURE.md:156 /
OPERATIONS.md:52 ("`devflow runner` … hidden `__runner` subcommand") — copy that sentence form for `devflow ask`. A
documented `.devflow/…` registration path needs a matching source constructor (doc_check.rs:391-399).

### Named-test gates
`cargo nextest run -p <pkg> <--lib|--bin devflow|--test ask_e2e> --no-tests=fail -E 'test(=module::path::name)'`, one
per command, chained with `&&` (CLAUDE.md Repo Traps).

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| `crates/devflow-cli/src/ask_server.rs` (JSON-RPC read/dispatch loop only) | service | request-response over stdio | No code in either crate reads stdin or speaks JSON-RPC; use RESEARCH.md Code Example 1 and the MCP 2025-11-25 spec. Threading, gate wait and notify do have analogs (above). |

## Metadata

**Analog search scope:** `crates/devflow-core/src/{gates,prompt,events,history,stop,liveness,doc_check,verify,agent_result,canary}.rs`, `crates/devflow-core/src/agents/`, `crates/devflow-cli/src/{main,commands,pipeline_gate,pipeline_launch,pipeline_outcomes,preflight,test_support}.rs`, `crates/devflow-cli/tests/{runner_e2e.rs,common/mod.rs}`, `doc-check-allowlist.toml`, ARCHITECTURE.md, OPERATIONS.md
**Files scanned:** 24
**Pattern extraction date:** 2026-11-18
