# Phase 41: Antigravity Driver - Context

**Gathered:** 2026-08-19
**Revised:** 2026-08-20 — adversarial review (claude opus / codex gpt-5.6-terra /
antigravity gemini-3.7-flash-high, all against live `agy` → `antigravity-cli` **1.1.16** and the real
source) BLOCKED the as-written plans. **D-02 and D-03 are re-opened (falsified) and re-derived
below.** Do not execute any pre-review version of 41-01 / 41-02.
**Status:** Ready for re-planning

Review evidence: `.planning/reviews/phase-41/SUMMARY.md` (synthesis) + `claude.md`, `codex.md`,
`antigravity.md` (individual transcripts).

## Phase Boundary

Phase 41 delivers the Antigravity driver: `devflow start --agent antigravity` launches the
Antigravity CLI headless and drives a stage to completion with honest completion detection. It also
closes two dogfood-hygiene items surfaced by the Phase 40 run — the leaked test monitors (HYG-01)
and the in-container git failures (HYG-02). Requirements: ANTG-01, ANTG-02, ANTG-03, HYG-01, HYG-02.

The rework adds four files to 41-01 scope that the falsified plans omitted — `agent_result.rs`
(the missing D-03 parser), `monitor.rs` (agent-aware first-turn schema), `pipeline_launch.rs`
(stream-launch routing), and `commands.rs` (the `devflow doctor` entry) — and re-derives HYG-02
away from the fabricated "3 git-env tests under root" premise (see D-06).

## Implementation Decisions

### Agent binary & launch
- **D-01: The driver targets the `agy` binary. [VERIFIED SOUND by all three reviewers]** `agy` is
  the operator's single, canonical Antigravity entry point — a shell wrapper
  (`exec antigravity-cli --dangerously-skip-permissions "$@"`). The conflicting `antigravity`
  (1.1.13) and `agycli` binaries are absent from PATH. The wrapper injects
  `--dangerously-skip-permissions` itself, so the driver argv must not add it again.
  Version note: the wrapper has drifted 1.1.14 (ROADMAP) → 1.1.15 (pre-review docs) → **1.1.16
  (live at review)**. Nothing pins a version anywhere; D-04 makes that acceptable by design, and
  T-41-03's "locks the wrapper to a vetted version" wording has been corrected (it does not lock).
- **D-02 (RE-OPENED — FALSIFIED; re-derived): Stream-json launch, NOT `-p`, with an
  Antigravity-shaped first turn.** — **Reversibility:** costly — undo would re-derive completion
  parsing and relaunch wiring.
  - **Falsified claim (pre-review):** `build_command` returned
    `agy -p --input-format stream-json --output-format stream-json` and reused
    `monitor::user_turn_line`. Both halves are wrong against the live CLI.
  - **`-p` is a Go-flag STRING flag requiring an argument, not a boolean.** It consumes the next
    token as the prompt. The planned argv makes `-p` swallow `--input-format`: the CLI prints
    prose, ignores stdin, and **exits 0** — silent failure. Negative controls (claude, live):
    bare `-p` → `flag needs an argument: -p`; `-p "<prompt>"` with stream-json input → "a prompt
    given on the command line would be ignored" (mutually exclusive). "Mirroring ClaudeDriver"
    was the error: `claude`'s `-p` is boolean, `antigravity-cli`'s is not.
  - **RE-DERIVED argv:** `build_command` returns
    `("agy", vec!["--input-format", "stream-json", "--output-format", "stream-json"])` — **no
    `-p`**, no `--dangerously-skip-permissions` (D-01). STACK.md's argv table ("still accurate"
    per the pre-review doc) is **wrong** on `-p` and stale on the version; see Canonical
    References.
  - **First-turn stdin schema is agent-specific.** DevFlow's `monitor::user_turn_line`
    (monitor.rs:726) emits `{"type":"user","message":{...}}` — the CLI rejects it outright:
    `stream input message is missing the "event" field`. The working shape is
    `{"event":"user","message":{...}}`. 41-01 adds an agent-aware variant (e.g.
    `user_turn_line_for(agent, prompt)`), keeping the `type`-key shape for Claude and emitting
    the `event`-key shape for Antigravity.
- **D-06 (NEW, replaces the falsified HYG-02 premise): The in-container failure is a worktree
  mount problem, not a uid problem, and not "3 git-env tests under root".** Verified by claude in
  the pinned image (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`) both ways: the
  **worktree** mounted into the container fails (`git check-ignore` → `fatal: not a git
  repository`, exit 128); the **main checkout** as uid 0 **passes**. Cause: a worktree's `.git`
  is a file (`gitdir: /var/home/denniyahh/Github/devflow/.git/worktrees/phase-41`) and
  `check-in-container.sh:17,69` mounts only `git rev-parse --show-toplevel` — the gitdir target
  lands outside the container mount. None of `gitignore_coverage.rs` / `ci_parity_guards.rs` /
  `pre_commit_branch_guard.rs` uses `git config --global` (zero matches); `ci_parity_guards.rs`
  makes no git calls at all; `gitignore_coverage.rs` runs only `git check-ignore`;
  `pre_commit_branch_guard.rs` reads `scripts/hooks/pre-commit` and runs `git symbolic-ref`.
  **The fix belongs in `check-in-container.sh` (or running from the main checkout), NOT in the
  three test files, and NOT as `skip_if_root()`** — GitHub Actions runs root over a normal
  checkout where these tests currently pass and guard real regressions (CR-01 / WR-07,
  protected-branch hook). A root skip would silently disable them on the only environment that
  matters. See 41-02.

### Completion detection
- **D-03 (RE-OPENED — FALSIFIED; re-derived): Antigravity needs its own stream-json parser —
  none exists, and the plan had no task implementing it.**
  - **Falsified claim (pre-review):** "parse the final stream-json `result` message" — with the
    trait default `parse_completion` returning `None` (agents/mod.rs:92-94) and no task in 41-01
    overriding it, the phase's stated goal (ANTG-03 honest completion) had **zero implementation
    behind it**.
  - **Live stream shape (all reviewers + Hermes smoke):**
    `{"event":"init",...}` → `{"event":"step_update",...}` → `{"event":"result","result":{"status":"SUCCESS","response":"DEVFLOW_RESULT: ..."}}`.
    Events carry an **`event` key**; Claude events carry `type`/`subtype`. DevFlow's
    `is_claude_event_stream` (agent_result.rs:862) requires `type:"system"` + `subtype:"init"` →
    gate false for Antigravity; `parse_marker_lines` requires `DEVFLOW_RESULT:` at line start,
    but the marker is JSON-escaped inside `result.response` → no match. Claude's own doc comment
    (agent_result.rs:1476-1479) explains why the Claude extractor exists — Antigravity needs the
    equivalent.
  - **RE-DERIVED:** add `is_antigravity_event_stream` (gate on the `event` key's `init`) and
    `parse_antigravity_event_result` (extract the last `result` event's `response` string, feed it
    to `parse_marker_lines`) in `crates/devflow-core/src/agent_result.rs`, mirroring
    `parse_claude_event_result`, and wire both into `evaluate_layer1`'s `.or_else` chain
    (agent_result.rs:1866) like `parse_codex_event_result`. Honest process-exit fallback
    (Layer 2) stays. **A marker-less stream never advances a commit-gated stage (ANTG-03)** —
    regression-tested, gated at `Stage::Plan` (the first commit-gated stage; `evaluate_layer2`
    sets `commit_gated = matches!(stage, Stage::Plan | Stage::Code)`, agent_result.rs:2037).
    Define is NOT commit-gated and legitimately advances on exit 0 — the pre-review plan asserted
    the wrong stage.
  - Wiring note: the trait hook `parse_completion` has **no call sites today** (checked: only
    the default and codex.rs:74 exist; `evaluate_layer1` dispatches parsers by name). The driver
    should still override `parse_completion` to delegate to the new parser (contract
    completeness), but the load-bearing dispatch is the `evaluate_layer1` chain.
  - **Transport wiring (falsified-adjacent):** `claude_stream_launch_enabled`
    (pipeline_launch.rs:707-713) is hardcoded to `agent == AgentKind::Claude`; any other agent
    resolves to `MonitorLaunch::Legacy` (pipeline_launch.rs:192-214), whose stdin is NOT the
    stream-turn channel — an Antigravity child would never receive the prompt. 41-01 must widen
    the stream-launch predicate to include `AgentKind::Antigravity` (keeping `STREAM_JSON_STAGES`
    gating) so `resolve_launch_shape` returns `PipeOwning { prompt }`.

### Health / preflight
- **D-04 (REVISED — intent kept, scope widened): Presence-only health check.**
  `ensure_agent_binary` / `agent_program` (preflight.rs:84-97) report Antigravity as installed
  when `agy` is on `PATH` — this half already works with no driver change. **`devflow doctor`
  does NOT:** its check list is a hardcoded `vec![]` at commands.rs:2286-2325 (git/sh/cargo/gh/
  claude/codex/opencode/pi/pi_subagent) with no Antigravity entry, and neither pre-review plan
  touched commands.rs — ROADMAP criterion 1 ("doctor reports it installed") was unachievable with
  no failing test. 41-01 adds the commands.rs entry. The planned `health() -> Ok(())` alone is
  trait-default **dead code**, not a check — keep it only as the trait default, and let the real
  presence gate be preflight + doctor. — **Reversibility:** reversible — adding a version floor
  later is a small, additive change. The marker-less contract (D-03) is the functional backstop:
  a wrong/stale binary fails the run honestly rather than advancing.

### Prompt rendering
- **D-05: Reuse `render_claude_style`. [VERIFIED SOUND by all three reviewers]** Antigravity is
  the Claude driver family (stream-json, same agentic loop); `contract_checks` asserts
  `prompt.contains("DEVFLOW_RESULT")` for all five stages and `ClaudeDriver.render_prompt` is
  byte-identical to `render_claude_style` (agents/mod.rs:225-228), which already passes. No
  dedicated renderer until a live probe shows the Claude framing is wrong.

## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope
- `.planning/ROADMAP.md` § "Phase 41: Antigravity Driver" — goal, success criteria (ANTG + HYG).
  Criteria 2 and 6 were corrected 2026-08-20 (version 1.1.14 → 1.1.16 live; HYG-02 reworded away
  from the "3 git-env tests under root" premise).
- `.planning/REQUIREMENTS.md` — ANTG-01..03, HYG-01, HYG-02 definitions. **NOTE: ANTG-02's
  "Claude-style `-p`" wording is superseded** by re-derived D-02 (no `-p`; event-key stdin
  schema). HYG-02's "3 git-env tests that fail as root" wording is superseded by D-06.

### Driver contract & patterns to mirror
- `crates/devflow-core/src/agents/claude.rs` — `ClaudeDriver` (stream-json `build_command`,
  `render_claude_style`) — the launch pattern to mirror EXCEPT the `-p` boolean (D-02).
- `crates/devflow-core/src/agents/mod.rs` — `AgentDriver` trait (incl. the `parse_completion`
  default that returns `None`), `driver_for`, the shared conformance suite (`contract_checks`
  returns **7** results — 1 name + 5 per-stage prompt + 1 program; the driver must be **enrolled
  in the `every_driver_passes_the_conformance_suite` array** at agents/mod.rs:274-279, which is
  hardcoded to 4 drivers today).
- `crates/devflow-core/src/agent_result.rs` — `parse_claude_event_result` +
  `is_claude_event_stream` (the model for the Antigravity parser), `evaluate_layer1` dispatch
  chain, `evaluate_layer2` (`commit_gated = Plan | Code`).
- `crates/devflow-core/src/monitor.rs` — `user_turn_line` (Claude `type`-key shape; Antigravity
  needs the `event`-key variant), `PipeOwning` monitor arm.
- `crates/devflow-cli/src/pipeline_launch.rs` — `claude_stream_launch_enabled` +
  `resolve_launch_shape` (must route Antigravity to `PipeOwning`, not `Legacy`).
- `crates/devflow-cli/src/commands.rs` — the `devflow doctor` hardcoded check list
  (commands.rs:2286-2325; add the Antigravity/`agy` entry).
- `crates/devflow-core/src/state.rs` — `AgentKind` enum + `FromStr`/`Display`/`AgentParseError`
  (add the `Antigravity` variant).
- `ARCHITECTURE.md` § "Extension points — adding an agent" — the 7-step onboarding checklist.

### Research
- `.planning/research/STACK.md` — Antigravity CLI surface. **NOTE: the "binary-name resolution"
  section is superseded by D-01, and the argv/flags table is NOT accurate** — the `-p`/`--print`
  row is wrong (string flag requiring an argument, mutually exclusive with stream-json input) and
  the version row is stale (1.1.14). The `--input-format stream-json` / `--output-format
  stream-json` rows are correct.
- `.planning/phases/41-antigravity-driver/41-RESEARCH.md` / `41-PATTERNS.md` — **carry
  pre-review claims that are superseded** (the `-p` argv, the `Stage::Define`-unchanged marker-less
  assertion, `antigravity_driver::` verify commands, v1.1.15). Treat every `[VERIFIED:
  CONTEXT.md:…]` citation in them as stale; re-derived decisions live in THIS file and in the
  reworked 41-01/41-02 plans.

### Test pattern
- `crates/devflow-cli/tests/phase7_cli.rs` — the stub-PATH + `ENV_MUTEX` regression pattern, and
  specifically `pi_marker_less_run_does_not_advance` (phase7_cli.rs:1218-1254): marker-less exit-0
  advances Define legitimately and gates at **Plan** via `wait_for_gate` — the exact model for
  ANTG-03, including `wait_for_gate` (phase7_cli.rs:1206) and `state.gate_pending` assertion.
- `crates/devflow-cli/src/test_support.rs` — `ReapMonitorOnDrop` (test_support.rs:573), the
  PID-specific, unwind-safe monitor reaper (`terminate_and_verify`: bounded TERM→KILL with
  verified death) — the model for HYG-01. NOTE: `test_support` is a module of the **binary** crate
  (no `lib.rs`), so integration tests must mirror the pattern with public `devflow-core` APIs
  (`agent::terminate_and_verify`, `agent::agent_running`) — see 41-02.

## Existing Code Insights

### Reusable Assets
- `ClaudeDriver` — the stream-json launch to mirror (minus the `-p` boolean).
- `parse_claude_event_result` / `is_claude_event_stream` — the parser pair to clone for the
  `event`-key framing (D-03).
- `monitor::user_turn_line` — the stdin turn writer (needs the agent-aware variant, D-02).
- `wait_for_gate` (phase7_cli.rs:1206) — the async-monitor-aware assertion helper the marker-less
  regression must use.
- `ReapMonitorOnDrop` (test_support.rs:573) + `devflow_core::agent::terminate_and_verify` /
  `agent_running` / `discover_stray_devflow_processes` — the per-PID reaping primitives (HYG-01).
- `DriverCapabilities` / `SandboxRequirements` / `DriverHealth` (`#[non_exhaustive] + Default`) —
  carry everything a new driver needs; no new crate deps (STACK.md).

### Established Patterns
- Marker-less never advances a **commit-gated** stage (Layer 1/2/3 completion machinery) — feeds
  D-03's regression test at `Stage::Plan`.
- `ensure_agent_binary` preflight fails loud when a configured agent binary is absent.
- Stream-launch routing is a per-agent predicate in `pipeline_launch.rs` (currently Claude-only).

### Integration Points
- `AgentKind` variant + `FromStr`/`Display` (`state.rs`).
- `driver_for` match arm + conformance-array enrollment (`agents/mod.rs`).
- `evaluate_layer1` dispatch chain (`agent_result.rs`).
- Stream-launch predicate + `resolve_launch_shape` (`pipeline_launch.rs`).
- `devflow doctor` check list (`commands.rs`).
- `agent_program` resolution (used by `ensure_agent_binary` + `devflow doctor`).

## Specific Ideas

No specific requirements — open to standard approaches.

## Deferred Ideas

- **Version floor / capability probe on `agy`** (GA-4 option A) — considered, not chosen. Revisit if
  `devflow doctor` accuracy matters or a stale binary regresses.
- **Update `research/STACK.md`** (binary-resolution section AND the argv table) and refresh
  `41-RESEARCH.md` / `41-PATTERNS.md` — deferred to a docs-cleanup pass; the reworked plans mark
  the stale claims superseded.
- **Open questions from the review (do not block):** (a) whether `step_update` `text_delta` is
  cumulative or delta-only — the parser keys off the `result` event's `response` field, so this
  does not block; probe with a multi-chunk run before ever relying on `text_delta`. (b) whether
  `{"event":"user",...}` is the canonical input schema or one accepted shape — probe
  `--json-schema` before treating it as a contract. (c) the container-run figure behind HYG-02 —
  re-derived by 41-02 from a real run against the main checkout.

---

*Phase: 41-Antigravity Driver*
*Context gathered: 2026-08-19 · revised: 2026-08-20 (adversarial review rework)*
