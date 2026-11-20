---
phase: 53
phase_name: "The Question Channel"
project: "DevFlow"
generated: "2026-11-20"
counts:
  decisions: 8
  lessons: 7
  patterns: 7
  surprises: 6
missing_artifacts: []
---

# Phase 53 Learnings: The Question Channel

## Decisions

### Ask server is a hidden stdio MCP subcommand that does gate work itself
`devflow_ask` is served by a hidden `devflow` subcommand speaking minimal MCP over stdio (`initialize`, `tools/list`, `tools/call`). It opens, registers and closes the gate through the existing `Gates` API, with a new `GateKind::Question` and a new `devflow gate answer` verb.

**Rationale:** Reuses the gate machinery, so notify, history and sweep behave like every other gate.
**Source:** 53-CONTEXT.md (D-03, D-05, D-09, D-10)

---

### Remove positional self-resolution of checkpoints
The relaunch that auto-decided checkpoints, its four devflow-core readers, `spawn_agent_and_record` and `rescan_gate_context` were deleted. A failed stage declaring `checkpoint: blocking-human` now opens a failure gate naming `devflow_ask`, for every agent and stage; Validate bypasses the fix loop.

**Rationale:** The old instruction contradicted `devflow_ask`; the declaration stays only as a fallback.
**Source:** 53-03-SUMMARY.md, STATE.md (53-03), 53-CONTEXT.md (D-12, D-13)

---

### Codex holds a call about 15 minutes, then returns `pending`; the agent re-calls by question_id
Operator answer OD-1 (b). A call capped before its own gate opens also replies `pending` with its question_id, and the question is kept without a gate until a re-call. The idle guard bounds continuous silence at 960 s while the question gate is open.

**Rationale:** An unanswered long block cost about 1.35 M input tokens in 15 min on codex polling, and could reach order 10^8 with a 3-day timeout.
**Source:** 53-REVIEWS.md (OD-1), 53-13-SUMMARY.md, STATE.md (53-13)

---

### Register pi as the second non-claude/codex registration; refuse opencode, hermes and antigravity
Operator answer OD-2. Those three agents are refused at launch with a message prefixed `refusing to launch `<agent>`:` that names the CLI agent name and `devflow_ask`. `--mode auto` still loses its non-claude fallback.

**Rationale:** pi has built-in MCP support; the others were out of reach for this phase (tracked as 999.248).
**Source:** 53-REVIEWS.md (OD-2), 53-08-SUMMARY.md, 53-14-SUMMARY.md

---

### An operator abort on a question gate is committed by the ask server before it claims the answer
The server writes `abort_requested` first, then claims and closes the gate, and returns a tool error. The flag is consumed by `abort()` removing the state file. Resume records `closed_on_resume` for a leftover question gate and does not apply a bound abort.

**Rationale:** Committing after the claim leaves a kill window in which the backstop's `read_answer` no longer sees the answer.
**Source:** 53-09-SUMMARY.md, STATE.md (53-09)

---

### Add `QuestionSpec::from_call_arguments` instead of changing `from_arguments`
The re-call's question_id is parsed by a new additive function; `from_arguments` and `QuestionError` are unchanged.

**Rationale:** Changing a pub signature in the published devflow-core is a semver break against the plan's start commit.
**Source:** 53-13-SUMMARY.md

---

### Keep `verify::phase_checkpoint_declarations` and `CheckpointDeclaration`
The must_have said to delete them, but the plan's own action deletes only if nothing else uses them, and `phase_has_human_only_checkpoint` (the unattended-launch refusal) still does. The override was accepted by the operator on 2026-11-19.

**Rationale:** Deleting them would remove that refusal.
**Source:** 53-10-SUMMARY.md, 53-UAT.md (item 3), 53-VERIFICATION.md

---

### pi registers through a per-launch `-e .devflow/ask-NN.mcp.ts` extension; the owned-file rule was not built
The pi spike verdict was BUILD with `pi.registerMcpServer` under `--no-approve`. Because the file lives under `.devflow/`, no driver writes a registration file into the project tree, so the `.owned` sha256 record and its three tests were skipped and recorded as a scope deviation.

**Rationale:** The plan's own clause keeps 53-08 behaviour for files under `.devflow/`.
**Source:** 53-MCP-SPIKE.md, 53-14-SUMMARY.md

---

## Lessons

### A must_have can contradict its own action
53-10 listed two items as deleted while its action deleted them only if unused. The executor followed the action and the conflict went to the operator as an override.

**Context:** Plan must_haves versus task actions, 53-10.
**Source:** 53-10-SUMMARY.md, 53-UAT.md

---

### A scan written to the plan's letter can fail on code the plan requires
53-08 required exactly one `.ask_wiring(` call in non-test source, but the required conformance case also calls it. The scan was widened to both crates and asserts exactly two named sites plus one caller of `wire_ask_registration`, with a planted control.

**Context:** Call-site scan for T-53-21.
**Source:** 53-08-SUMMARY.md

---

### Deleting a feature leaves dead code that clippy `-D warnings` rejects
Removing the relaunch made `spawn_agent_and_record` and `rescan_gate_context` dead, so they and one test's only caller were deleted, touching two files outside the plan's list.

**Context:** 53-03 Task 1.
**Source:** 53-03-SUMMARY.md

---

### Test helpers that round-trip a struct can drop new fields
`backdate_gate` rewrote the gate file through `GateFile`, which does not name `question`, so a backdated question gate read as a plain gate. It now rewrites through `serde_json::Value`, and RED was re-verified by mutation.

**Context:** Adding a flattened key to an on-disk format.
**Source:** 53-04-SUMMARY.md

---

### A negative control can pass vacuously when the thing it checks was deleted
The no-fallback test asserted the agy stub was never launched, which would pass trivially once agy was removed. It now asserts the codex stub, present on PATH in every rig, ran no stage turn.

**Context:** Failover e2e tests, 53-12.
**Source:** 53-12-SUMMARY.md

---

### A flaky monitor test appeared once under `check.sh all`
`monitor::tests::both_arms_meet_every_supervisor_responsibility_with_a_non_claude_adapter` failed once with a `SigBlk` mismatch and passed on every rerun. The summary guesses a `/proc/<pid>/status` read between fork and exec; a read-after-exec check would confirm it. It was logged in deferred-items.md, not fixed.

**Context:** Found by 53-07, in a file 53-07 does not touch.
**Source:** 53-07-SUMMARY.md, deferred-items.md

---

### Commit subjects over 72 characters recurred
Several plans amended an unpushed commit after the commit-msg hook warned (53-07, 53-11, 53-13, 53-08).

**Context:** Conventional Commit length rule.
**Source:** 53-07-SUMMARY.md, 53-11-SUMMARY.md, 53-13-SUMMARY.md

---

## Patterns

### Verify-before-build spike gating a driver
Each non-claude registration was spiked first (held call, error mode, config isolation) and given a verdict line before code was written; pi's was `BUILD`.

**When to use:** Any driver whose MCP behaviour is not documented.
**Source:** 53-MCP-SPIKE.md, 53-14-SUMMARY.md

---

### Mutation or planted control to show RED
Where tests were written alongside the code, RED was shown by mutating the code (53-04, 53-08), and scans carry a planted extra site that must be reported.

**When to use:** When strict test-first ordering was not followed.
**Source:** 53-04-SUMMARY.md, 53-08-SUMMARY.md

---

### Control test that passes at RED by design
`a_failure_without_a_declaration_has_no_checkpoint_sentence` passed at RED because it pins the path the change must leave alone; the other two failed at RED.

**When to use:** Changes that must leave a neighbouring path untouched.
**Source:** 53-03-SUMMARY.md

---

### Additive API instead of a signature change on a published crate
`from_call_arguments` was added beside `from_arguments`; `HolderIdentity` was made `pub`, additive, to fix a clippy `type_complexity`.

**When to use:** Any change to devflow-core's pub surface.
**Source:** 53-13-SUMMARY.md, 53-07-SUMMARY.md

---

### Duplicate resolutions are shown, not applied
A second answer or close for one question_id keeps its history line and gets an `ignored <id>` line, so the view never shows both an answer and a close.

**When to use:** Event timelines where two resolvers can race.
**Source:** 53-02-SUMMARY.md, STATE.md (53-02)

---

### Strict wire-format parsing with explicit absent/null rules
A JSON null optional field reads as absent, a wrong-typed field is refused naming the field, blank or whitespace-only option ids are refused, and `kind` parses through the enum's serde names.

**When to use:** Parsing agent-supplied tool arguments.
**Source:** 53-05-SUMMARY.md

---

### Stubs write beside themselves, not in the project tree
The launch e2e stubs write `answer.txt` in their own fake-bin dir so the Plan stage sees no untracked file.

**When to use:** E2E fixtures that run a stage in a temp repo.
**Source:** 53-08-SUMMARY.md

---

## Surprises

### Abort clears the state file, so the planned commit and assertion could not run
`pipeline_gate::abort` ends with `clear_state`; the plan's state commit after abort had nothing to commit and a later `update_state` would fail with `MissingState`. In 53-03 the same clear removed the state a test meant to read.

**Impact:** Both plans assumed state survives abort.
**Source:** 53-09-SUMMARY.md, 53-03-SUMMARY.md

---

### The Condvar queue was needed one task earlier than planned
The 53-01 `mpsc` worker could not wake on EOF or a signal while the reader stayed blocked on stdin, so Task 2 of 53-07 moved the reader to its own thread and introduced the `Mutex<VecDeque>` plus `Condvar`.

**Impact:** Held-call server design.
**Source:** 53-07-SUMMARY.md

---

### pi refuses an extension named `*.mcp.json`
pi parses it as JSON and exits 1, so the per-launch file is `.mcp.ts`; `.gitignore` and `gitignore_coverage` gained `.devflow/ask-*.mcp.ts`.

**Impact:** Found in the pi spike.
**Source:** 53-MCP-SPIKE.md, 53-14-SUMMARY.md

---

### Codex polls a held call about every 21 s
That polling cost about 1.35 M input tokens in 15 minutes, which forced the cap and re-call design.

**Impact:** Raised in plan review against the spike's own note.
**Source:** 53-REVIEWS.md

---

### Two reviewer findings about fabricated operator answers were false
Both answers were real, from the operator's `gate reject --note` of 2026-11-18, recorded in `.devflow/events.jsonl`, which was not in the reviewed snapshot. The fix was to cite the note in 53-06 and 53-08.

**Impact:** A reviewer snapshot that omits run state.
**Source:** 53-REVIEWS.md

---

### Live proofs held a question for hours and passed
claude held 16,306 s with one Code launch; codex returned `pending` at the 900 s cap and the re-call by question_id carried the answer after 1,078 s, with codex-config before equal to after. The M4 run on a throwaway project recorded two GSD plan-phase questions in `devflow history` as `operator_via_gate`.

**Impact:** Run exceptions: the proof ran from a separate install root because the phase's own runner was live at Validate (999.254).
**Source:** 53-UAT.md, 53-PROOF-RUN.md
