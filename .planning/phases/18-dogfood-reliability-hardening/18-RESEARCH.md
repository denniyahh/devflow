# Phase 18: Dogfood Reliability Hardening - Research

**Researched:** 2026-07-20
**Domain:** Internal state-machine / completion-protocol bug fixes in a Rust CLI (devflow-core, devflow-cli) — not a greenfield feature domain
**Confidence:** HIGH

## Summary

This is a source-grounded verification phase, not a technology-discovery phase. All seven
items (18a–18g) were re-verified directly against current HEAD (workspace clean, `cargo test
--workspace` 380 passed / 0 failed, `cargo clippy --workspace --all-targets -- -D warnings`
exit 0, `cargo fmt --check` exit 0 — matching 17-REVIEW.md's Round 5 baseline). **Every defect
cited in CONTEXT.md still reproduces at current HEAD**, at line numbers that have shifted only
slightly (the file grew from the 17-REVIEW.md diff baseline). None of the seven items are
already fixed — unlike the WR-06 stale-ROADMAP incident (19e/19f), there is no false-positive
risk here to flag.

Two structural facts drive the plan shape: (1) **18d and 18e are causally entangled** —
18e's Layer-0-discards-verdict bug only becomes an *unbounded* loop because of 18d's
counter-reset bug, and 18d's counter-reset bug only becomes *user-visible as a real incident*
because 18e defeats Validate for `external_verify` PLANs (currently zero of them, which is why
the test suite is silent about both). They should ship in the same wave, fixed together, with
tests that exercise the combined scenario. (2) **18a must land before 18b** — 18b's monitor
liveness state is documented in CONTEXT.md as extending 18a's reconciliation rather than
duplicating it, and 18a's shape (what `doctor` diffs and how it reports) should be settled
before adding a second tracked PID to it.

**Primary recommendation:** sequence as three waves — Wave A: 18a → 18b (doctor reconciliation,
then monitor liveness); Wave B: 18d + 18e together (Code↔Validate safety-gate reachability);
Wave C: 18c, 18f, 18g (each independent of the others and of Wave A/B). Every fix in this phase
follows an existing in-repo pattern (execution_root vs. project_root split, RED-then-GREEN
regression tests, `[never-silent]` gate idiom) — none require new external research.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| `devflow doctor` reconciliation (18a) | CLI (devflow-cli) | Core (devflow-core: state/events/gates read APIs) | `doctor()` lives in `main.rs`; it composes existing devflow-core read-only inspection primitives (`workflow::list_states`, `events::last_event_for_phase`, `Gates::list_open`, `crate::agent::agent_running`) — no new core module needed, this is a CLI-side aggregation over pre-existing core APIs |
| Monitor liveness tracking (18b) | Core (devflow-core: state.rs, monitor.rs) | CLI (devflow-cli: status/doctor rendering) | The pid must be persisted to `State` (core) at spawn time (monitor.rs / launch_stage) for it to survive process restarts; rendering it in `status`/`doctor` is CLI-side, same split as the existing `agent_pid` |
| Worktree-aware staleness (18c) | CLI (devflow-cli: main.rs staleness functions) | — | `enforce_build_staleness` and its helpers are CLI-local (not exported from devflow-core); the fix threads `state.worktree_path` through functions that already take `state: &State`, no cross-tier work |
| Code↔Validate loop bound (18d) | CLI (devflow-cli: main.rs `transition`) | Core (devflow-core: mode.rs `MAX_CONSECUTIVE_FAILURES`) | `transition()` (state mutation + reset) lives in main.rs; the ceiling constant and gate policy (`should_gate`) live in devflow-core's `mode.rs` — the fix must keep both in sync |
| Layer 0/Validate verdict reconciliation (18e) | Core (devflow-core: agent_result.rs) | CLI (devflow-cli: main.rs `advance()` dispatch) | The four-layer cascade and `evaluate_layer0`/`evaluate_layer1` live entirely in devflow-core; `advance()`'s `passed = matches!(result.verdict, ...)` in main.rs is the consumer that must also change if the gate-vs-advance decision needs a third outcome |
| Preflight gate re-run bound (18f) | CLI (devflow-cli: main.rs `run_preflight`/`launch_stage`) | Core (devflow-core: gates.rs `GateAction`) | `run_preflight`'s recursive `launch_stage` call is CLI-local; `GateAction` (Advance/LoopBack/Abort) is defined in devflow-core's gates.rs and is not stage-aware today — an "already adjudicated" signal must either ride through `State` (core) or stay CLI-local as a parameter |
| WR-03 test stabilization (18g) | Test-only (devflow-cli/tests) | — | No production code changes; the fix is entirely inside `crates/devflow-cli/tests/phase7_cli.rs` |

## Project Constraints (from CLAUDE.md)

No project-local `./CLAUDE.md` exists in this repository (checked: absent). The user's global
`~/.claude/CLAUDE.md` and `~/.claude/rules/{git-workflow,code-style}.md` apply as personal
defaults, most relevantly for this Rust phase:

- `cargo clippy -- -D warnings` and `cargo fmt` before every commit (global rule: Rust section) —
  this project's own CI already enforces the stricter `--workspace --all-targets` form
  (17-REVIEW.md WR-08/WR-10 already found and partially fixed a scope gap here; do not
  regress it).
- Use `?` for error propagation, not `.unwrap()`, except in tests — the existing codebase
  already follows this convention throughout `main.rs`/`agent_result.rs`; new code in this
  phase must match.
- Conventional commit messages (`type(scope): description`, imperative, ≤72 chars) — this
  project's own git history already follows this convention (see `git log` above); continue it,
  scope `(18a)`…`(18g)` per item.
- Surgical changes: touch only what each fix requires. Given the shared files (main.rs is the
  single largest touch point for 18a/18c/18d/18e/18f), the planner should sequence tasks to
  minimize simultaneous edits to overlapping line ranges within a wave.
- Every fix should be RED-proven (write a failing test against current behavior, then make it
  pass) per this project's own established pattern (see every `17-NN-SUMMARY.md`).

<user_constraints>
## User Constraints (from CONTEXT.md)

CONTEXT.md for this phase is structured as a ROADMAP-style item list (18a–18g) rather than the
usual Decisions/Discretion/Deferred format, because this phase was scoped by direct operator
reprioritization rather than through `/gsd-discuss-phase`. Two explicit, dated, BINDING operator
decisions are recorded inline and reproduced verbatim below. There is no separate
"Claude's Discretion" or "Deferred Ideas" section — treat everything in 18a–18g as in-scope and
everything in "Explicitly Out of Scope (moved to Backlog)" as out of scope.

### Locked Decisions

**18e (2026-07-20, operator):** gate only when ambiguous, not on every declared
`external_verify`. Advance automatically when the probe passes AND the agent's `DEVFLOW_RESULT`
carries `verdict: pass` — two independent signals agreeing. Gate for a human when they disagree,
or when the probe passes but no verdict arrived at all. Rejected: always gating on any declared
`external_verify` (correct but removes unattended operation for the exact PLANs that declared a
probe *in order to* run unattended).

**18f (2026-07-20, operator):** treat `GateAction::Advance` on a preflight gate as an explicit
override that SKIPS `run_preflight` entirely — the check has already been adjudicated by the
human. `GateAction::LoopBack` keeps re-running the check, since that path means "I will fix it,
then retry" and the state may genuinely have changed. Bound the recursion regardless as a
backstop.

### Scope (from ROADMAP.md Phase 18 item list)

- 18a: `devflow doctor` project-aware reconciliation (was 18d). Sequence before 18b.
- 18b: monitor liveness observability (was 19a; extends 18a).
- 18c: staleness evaluated against the wrong tree (was 19d; root cause of Round 4 CR-01).
- 18d: Code↔Validate `consecutive_failures` reset makes `MAX_CONSECUTIVE_FAILURES` unreachable
  (was 19g).
- 18e: Layer 0 short-circuit makes Validate unpassable when `external_verify` is declared
  (was 19k). See binding decision above.
- 18f: approving a preflight gate re-runs the identical check and wedges for 7 days (was 19l).
  See binding decision above.
- 18g: WR-03 test stabilization (was 18e in the old numbering).

### Deferred Ideas (OUT OF SCOPE — moved to Backlog, dirs `999.1`–`999.5`)

- Hermes support (`HermesAgent` adapter, skill rewrite, plugin) — `999.1`.
- Two-process-per-phase tracking model (19b) — `999.2`.
- CLI operator discoverability (`gate show`, in-stage progress, discoverable recovery verbs,
  19c) — `999.3`. Note: `devflow gate show <phase>` and rate-limit-reset surfacing
  (OPERATOR-OBSERVABILITY-FINDINGS.md Finding 3) live here — do NOT pull them into 18a/18b even
  though they're adjacent; only the reconciliation/liveness *data model* is in scope for 18.
- Version-tag contention on concurrent ship (19h) — `999.4`.
- `ChangelogAppend` placeholder content (19j) — `999.5`.
- Already resolved, not carried forward: 19i (PATH race, `96411eb`/`40dade3`); 19e/19f
  (`write_version` trailing comma, changelog/tag desync — closed by 17-13's `12b5b98`/`e421ebd`).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| 18a | `devflow doctor` project-aware reconciliation | Confirmed `_project_root` unused at `main.rs:3119`; existing devflow-core read primitives to compose (`workflow::list_states`, `events::last_event_for_phase`, `Gates::list_open`, `agent::agent_running`) identified below |
| 18b | Monitor liveness observability | Confirmed no `monitor_pid` field in `state.rs`; confirmed `main.rs:1216` emits it to the event log only; `status` (`main.rs:2592`) probes only `agent_pid`; fix shape (persist + probe) identified below |
| 18c | Worktree-aware staleness enforcement | Confirmed `enforce_build_staleness`/`embedded_commit_is_stale`/`combined_staleness`/`is_self_dogfood_workspace` all run `git` with `current_dir(project_root)`, ignoring `state.worktree_path`; existing `execution_root` vs. `project_root` split precedent identified in `evaluate_layer0` |
| 18d | Code↔Validate safety-gate reachability | Confirmed `transition()` (`main.rs:1788-1811`) unconditionally zeroes `consecutive_failures` AND `infra_failures`; confirmed the exact Code→Validate transition (`main.rs:1353`) is the one crossed every retry cycle; assessed whether `infra_failures` inherits the same weakness (partial — see Pitfall 2) |
| 18e | Layer 0/Validate verdict reconciliation | Confirmed `evaluate_layer0`'s affirmative-success arm (`agent_result.rs:784-794`) always sets `verdict: None`; confirmed `advance()`'s `passed = matches!(result.verdict, Some(Verdict::Pass))` (`main.rs:1361`) then discards it; implementation shape for the binding decision identified below |
| 18f | Preflight gate re-run bound | Confirmed both `GateAction::Advance` and `GateAction::LoopBack` arms of `run_preflight` (`main.rs:796-825`) call `launch_stage(state, None, None)`, which unconditionally re-runs `run_preflight`; confirmed the existing `FailOnceAdapter` test fixture (`main.rs:4978-5013`) cannot reproduce the wedge (documented in its own comment) |
| 18g | WR-03 test stabilization | Confirmed `parallel_creates_two_worktrees_and_spawns_two_monitors` (`crates/devflow-cli/tests/phase7_cli.rs:152-201`) re-checks `phase7_stdout.exists()`/`phase8_stdout.exists()` after unrelated assertions; confirmed the exact race mechanism is already documented and already fixed for the sibling `wait_for_pid` helper in the same file (`phase7_cli.rs:101-105`) — reuse that pattern |
</phase_requirements>

## Standard Stack

Not applicable in the conventional sense — this phase adds zero new dependencies. Confirmed:
`crates/devflow-cli/Cargo.toml` and `crates/devflow-core/Cargo.toml` `[dependencies]` sections
were inspected; no crate is a candidate for addition or upgrade for any of 18a–18g. All fixes
are implemented with std library (`std::process::Command`, `std::fs`), the existing `serde`
derives already on `State`/`AgentResult`, and the existing `thiserror` error types.

### Core (existing, reused — no new versions to verify)
| Crate | Role in this phase |
|-------|--------------------|
| `devflow-core::state::State` | 18b adds a `monitor_pid: Option<u32>` field (serde-default, mirrors `worktree_path`'s `#[serde(default)]` pattern) |
| `devflow-core::agent_result` | 18e's fix site (`evaluate_layer0`/`evaluate_agent_result_inner`) |
| `devflow-core::gates::GateAction` | 18f's fix touches how `Advance`/`LoopBack` are dispatched in `run_preflight` |
| `devflow-core::mode` | 18d's `MAX_CONSECUTIVE_FAILURES`/`should_gate` — doc comment (`mode.rs:7-8`) is stale per WR-11 and must be corrected once 18d ships |
| `devflow-core::events` | 18a composes `last_event_for_phase` for reconciliation reporting |
| `devflow-core::recover` | 18a should reuse `recover::agent_pid_for`-style liveness probing rather than reimplementing it; `recover.rs`'s `RecoveryStatus`/`inspect_all` is the closest existing analog to what `doctor` needs to grow into |

### Package Legitimacy Audit

Not applicable — no external packages are installed, upgraded, or newly declared by this phase.
Skipping the legitimacy gate per the protocol's own scope (only required "whenever this phase
installs external packages").

## Architecture Patterns

### System Architecture Diagram

```
                    devflow start / resume / parallel
                              |
                              v
                     +-----------------+
                     |  launch_stage()  |  (main.rs:1137)
                     +-----------------+
                       |     |      |
      1. run_preflight |     |      | 3. spawn_monitor()
         (18f fix site)|     |      |    (monitor.rs) --> persists
                       v     |      |    monitor_pid to State (18b fix)
                 [preflight  |      v
                  gate?]     |  +---------------------+
                       |     |  | enforce_build_       |
              Advance/ |     |  | staleness()           |  (18c fix site:
              LoopBack |     |  | (main.rs:1092)        |   thread worktree_path
              re-runs  |     |  +---------------------+    through here)
              preflight|             |
              (BUG,    |             v
              fixed by |      agent CLI spawned (background monitor)
              18f) --->+             |
                              agent exits, monitor calls `devflow advance --phase N`
                              |
                              v
                     +------------------------+
                     | evaluate_agent_result() |  (agent_result.rs:799)
                     |  Layer 0 (probes) ------|--> affirmative Success,
                     |  Layer 1 (marker)   ^   |    verdict: None (BUG,
                     |  Layer 2 (exit+git) |   |    18e fixes this to
                     |  Layer 3 (fallback) |   |    consult Layer 1's
                     +----------------------+  |    verdict at Validate)
                              |                |
                              v                |
                     +------------------+      |
                     |  advance()        |------+
                     |  (main.rs:1277)   |
                     +------------------+
                       |            |
              Code success   Validate outcome
                       |            |
                       v            v
                +-------------+  +----------------------+
                | transition()|  | handle_validate_      |
                | (main.rs:   |  | outcome()              |
                |  1788) --   |  | consecutive_failures  |
                |  resets     |  | += 1 on fail (BUG:     |
                |  BOTH       |  | transition() below      |
                |  counters   |  | zeroes it every retry, |
                |  every call |  | 18d fixes this)        |
                |  (18d fix   |  +----------------------+
                |  site)      |      |
                +-------------+      v (fail) loop_back_to_code()
                       |              (no transition() call —
                       v               counter survives THIS hop,
                  next stage           wiped on the NEXT Code success)
                       |
                       v
                    Ship (always gates)

devflow doctor / devflow status         (18a/18b: read-only reconciliation layer)
        |
        +--> workflow::list_states()      (per-phase State, incl. new monitor_pid)
        +--> events::last_event_for_phase() (events.jsonl tail)
        +--> Gates::list_open()            (pending gate files)
        +--> agent::agent_running(pid)     (agent_pid liveness probe, existing)
        +--> agent::agent_running(monitor_pid) (NEW, 18b — the "who watches
                                                 the watcher" probe)
        +--> branch ancestry checks        (NEW, 18a — diff state.stage against
                                             what the worktree branch actually
                                             contains)
```

A reader can trace: `start` → `launch_stage` → preflight gate (18f fix boundary) → staleness
gate (18c fix boundary) → spawn_monitor (18b's persistence hook) → agent runs → `advance` →
four-layer evaluation (18e fix boundary) → `transition`/`handle_validate_outcome` (18d fix
boundary) → next stage or Ship. `doctor`/`status` (18a/18b) sit orthogonal to this flow as a
read-only diagnostic layer that reads the same on-disk artifacts (`State`, `events.jsonl`,
gate files, agent/monitor pid files) this flow writes.

### Recommended Project Structure

No new files or directories. All seven fixes land inside the existing module layout:

```
crates/devflow-cli/src/main.rs        # 18a (doctor), 18c, 18d, 18e (advance() dispatch), 18f
crates/devflow-core/src/state.rs      # 18b (new monitor_pid field)
crates/devflow-core/src/monitor.rs    # 18b (persist monitor_pid at spawn)
crates/devflow-core/src/agent_result.rs # 18e (evaluate_layer0 / evaluate_agent_result_inner)
crates/devflow-core/src/mode.rs       # 18d (doc-comment correction once fixed; possibly a new
                                       #      predicate if the reset needs to become conditional)
crates/devflow-core/src/recover.rs    # 18a (reuse candidate — do not duplicate agent_pid_for)
crates/devflow-cli/tests/phase7_cli.rs # 18g (test-only)
```

### Pattern 1: `execution_root` vs. `project_root` split (reuse for 18c)

**What:** Two distinct roots are threaded through completion-evaluation code: `project_root`
(the main checkout, where `.planning/` and `.devflow/` live) and `execution_root`/`worktree_path`
(where the phase's actual code changes live, when worktree mode is active). This split already
exists and is exercised for exactly this reason.

**When to use:** Any check whose answer depends on "what does the code under test currently
look like" (as opposed to "where does DevFlow's own bookkeeping live") must evaluate against
`execution_root`, not `project_root`.

**Example (existing code, the pattern 18c must copy):**
```rust
// Source: crates/devflow-core/src/agent_result.rs:713-719, 729
// Two roots are intentionally kept distinct: `project_root` is used to
// DISCOVER the PLAN's declared commands (`.planning/phases/` lives there,
// not in a worktree checkout), while `execution_root` — the worktree, when
// one is set — is where probes actually RUN.
let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
let commands = crate::verify::external_verify_commands(project_root, state.phase);
// ... commands are then run with execution_root, not project_root
```

**Applied to 18c:** `embedded_commit_is_stale`, `combined_staleness`,
`tree_has_modified_build_inputs`, and `is_self_dogfood_workspace` (the ancestry/dirty-tree
checks, NOT the self-dogfood-workspace-identity check, which is deliberately about
`project_root`'s own `Cargo.toml`) must accept and shell out to an `execution_root` derived the
same way: `state.worktree_path.as_deref().unwrap_or(project_root)`. `enforce_build_staleness`
already receives `state: &State` — no new parameter is needed at its call site, only at the
helper functions it calls, which currently take `project_root: &Path` positionally instead of
deriving `execution_root` from `state`.

### Pattern 2: RED-then-GREEN regression tests, named after the bug

**What:** Every prior phase's `SUMMARY.md` in this repo documents writing a failing test that
reproduces the defect BEFORE the fix, confirming it fails for the documented reason, then making
it pass. Test names directly name the invariant being pinned (e.g.
`transition_resets_infra_failures`, `run_preflight_advance_gate_launches_agent_exactly_once`).

**When to use:** Every one of 18a–18g except 18g (which is itself a test fix — the RED state
is "the existing test flakes," proven by a tight loop, see Validation Architecture below).

**Example (existing code, the pattern every fix here should follow):**
```rust
// Source: crates/devflow-cli/src/main.rs:4536 (transition_resets_infra_failures)
// This EXISTING test already proves transition() resets infra_failures —
// 18d's regression test is structurally identical but proves the ceiling
// is UNREACHABLE across repeated Code<->Validate cycles, e.g.:
//   for _ in 0..MAX_CONSECUTIVE_FAILURES {
//       handle_validate_outcome(root, &mut state, false).unwrap(); // fail, +1
//       transition(root, &mut state, Stage::Validate).unwrap();    // BUG: resets to 0
//   }
//   assert_eq!(state.consecutive_failures, 0); // RED: proves it never reaches 3
```

### Pattern 3: `[never-silent]` gate idiom (reuse for 18e's "gate for a human" arm)

**What:** Every failure path that needs a human decision routes through `run_gate` with a
`[never-silent]`-prefixed context string and returns a `GateAction` the caller dispatches on.
`handle_stage_failure` (`main.rs:1595-1623`) is the canonical shape: build a context string,
call `run_gate`, match on `Advance`/`LoopBack`/`Abort`.

**When to use:** 18e's "gate for a human when [probe and verdict] disagree, or when the probe
passes but no verdict arrived at all" is a NEW gate condition distinct from Validate's existing
`consecutive_failures`-gated retry loop. Reusing `handle_stage_failure`'s shape (rather than
routing through `handle_validate_outcome`'s counter-based logic) is the pattern-consistent
choice — see Common Pitfall 3 below for why routing through the counter-based path is the wrong
shape for an "ambiguous, gate immediately" outcome.

### Anti-Patterns to Avoid

- **Reusing `handle_validate_outcome`'s `passed: bool` signature for 18e's three-way outcome:**
  `passed` is a boolean; 18e's decision matrix is three-way (agree-pass / disagree / probe-only).
  Forcing a three-way outcome through a boolean drops information (see Common Pitfall 3).
- **Fixing 18d by simply removing the reset from `transition()` entirely:** `mode.rs`'s doc
  comment (lines 20-33) documents that the `infra_failures` reset is *intentional and correct*
  for its own ceiling's semantics (a stuck loop confined to repeated failures of the SAME stage
  never crosses a `transition()` call — see Common Pitfall 2). A blanket removal would break
  `transition_resets_infra_failures` (an existing, currently-correct regression test) and widen
  `MAX_INFRA_FAILURES`'s ceiling from "5 unobserved cycles" to "a phase's entire lifetime" for
  infra faults, which is the exact regression 17-06 was written to prevent. The fix must be
  scoped to `consecutive_failures` specifically, or must change WHEN the reset fires (e.g. only
  reset `consecutive_failures` on a transition OUT of Validate on a `passed: true` outcome), not
  blanket-remove the reset.
- **Making 18f's "skip preflight" override a global flag:** it must be scoped to the specific
  stage the gate was resolved for, mirroring `handle_stage_failure`'s existing
  `Gates::cleanup(project_root, state.phase, stage)` call (stage-scoped cleanup already exists;
  the skip signal should ride the same shape, not a process-global or phase-global bypass that
  could accidentally skip preflight for an unrelated later stage).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Monitor/agent liveness probing | A new `is_pid_alive` implementation | `crate::agent::agent_running(pid)` (already hardened — rejects pid `0` and values above `i32::MAX` per `OPERATOR-OBSERVABILITY-FINDINGS.md` Finding 1) | The existing probe is correct; the defect in 18b is that it's never called on `monitor_pid`, not that the probe itself is wrong |
| Reading the most recent event for a phase | Re-parsing `events.jsonl` inline in `doctor()` | `events::last_event_for_phase(project_root, phase)` (already used by `status`, `main.rs:2601`) | Already exists, already handles the JSONL tail-read correctly |
| Listing pending gates | Scanning `.devflow/gates/` directly | `Gates::list_open(project_root)` (already used by `status`'s `render_pending_gate_banner`) | Already exists and already handles the gate-file schema |
| Per-phase state enumeration | A new directory walk over `.devflow/state-*.json` | `workflow::list_states(project_root)` (already used by `recover::inspect_all` and `status`) | Already exists, already handles legacy single-slot migration |
| Ancestry/staleness git plumbing | New git subprocess wrapper | Reuse `embedded_commit_is_stale`'s existing `merge-base --is-ancestor` pattern, just parameterize the `current_dir` | The ancestry-exit-code contract (0/1/other) is already correctly handled including the reverse-probe for the Ahead case (Pitfall 4, documented at `main.rs:849-861`) — only the root needs to change, not the logic |

**Key insight:** Every one of 18a–18g has an existing, correct, reusable primitive somewhere
else in the codebase that does 80-100% of what the fix needs. This phase's risk is not "we lack
a building block" — it's "the building block exists but isn't wired to the right input" (18c's
staleness check has the right git logic pointed at the wrong root; 18b's liveness probe is
correct but never called on the right pid; 18a's `doctor` has the right shape as a CLI command
but the wrong scope). Treat every fix as a wiring/scoping change, not a new subsystem.

## Common Pitfalls

### Pitfall 1: Fixing 18e without 18d leaves a bounded-but-still-wrong loop (and vice versa)

**What goes wrong:** Ship 18e alone (Layer 0 correctly defers to Layer 1's verdict at Validate)
without 18d: an `external_verify` PLAN whose agent explicitly reports `verdict: gaps` at Validate
now correctly fails Validate (previously it silently always failed via `verdict: None`,
indistinguishable from a real gap) — but `handle_validate_outcome`'s `consecutive_failures`
counter STILL never reaches `MAX_CONSECUTIVE_FAILURES` because `transition()` still zeroes it
every Code→Validate hop. The loop is now *correctly evaluating* pass/fail but still runs forever
in Auto mode with no forced gate. Ship 18d alone without 18e: the counter now correctly reaches
3 and forces a gate — but the gate fires on a Validate stage whose `passed` is `false` for the
WRONG reason (Layer 0 discarding a real `verdict: pass`), so the human is asked to review a
false failure, not the real ambiguity 18e's decision matrix is meant to catch.

**Why it happens:** The two counters (`transition()`'s reset, `evaluate_layer0`'s verdict
discard) are independent bugs in independent modules (main.rs vs. agent_result.rs) that happen
to compound in the one scenario (`external_verify` declared at Validate) neither currently has
test coverage for, because no PLAN in this repo declares `external_verify` today.

**How to avoid:** Plan 18d and 18e as a single wave with a shared integration test that exercises
BOTH fixes together: an `external_verify`-declared Validate stage where the probe passes and the
agent reports `verdict: gaps` (disagreement — must gate, not loop silently and not falsely
advance), run through several Code↔Validate cycles, asserting `consecutive_failures` actually
increments and reaches the ceiling if the disagreement path is (by design or bug) NOT
immediately gating.

**Warning signs:** A plan that ships 18d or 18e in isolation without a test that declares
`external_verify` AND drives multiple Code↔Validate cycles.

### Pitfall 2: `infra_failures`' reset is not identically buggy to `consecutive_failures`' reset — don't blanket-fix both the same way

**What goes wrong:** `mode.rs`'s doc comment (lines 20-33, authored in 17-06) explicitly argues
the `infra_failures` reset in `transition()` is CORRECT: infra failures
(`ResourceKilled`/`AgentUnavailable`) are routed through `handle_infra_outcome` →
`gate_or_abort_infra` → `handle_stage_failure`, whose `Advance`/`LoopBack` arms call
`launch_stage` directly — NOT `transition()`. So a stuck loop confined to ONE stage repeatedly
failing infra-style never crosses a `transition()` call, and the reset only fires when that
stage genuinely succeeds and moves forward — which is the correct place to zero a per-stage
fault counter. This is verifiably different from `consecutive_failures`, whose failure path
(`handle_validate_outcome` on a Validate failure) ALSO does not call `transition()` directly
(it calls `loop_back_to_code`, which sets `state.stage = Code` without `transition()`) — but the
NEXT step, Code's own success, DOES call `transition(.., Stage::Validate)`, which is the same
function that owns the counter's ceiling. The two counters are asymmetric: `infra_failures`
accumulates within a single stage's repeated failures and resets on THAT stage's forward
progress (correct); `consecutive_failures` is meant to accumulate across repeated
Code↔Validate CYCLES and gets reset by an intermediate stage's (Code's) unrelated forward
progress (incorrect).

**Why it happens:** `infra_failures` and `consecutive_failures` were designed to answer
different questions ("has this ONE stage failed too many times" vs. "has this STAGE PAIR
cycled too many times") but share one reset call site because 17-06 added `infra_failures`
next to the existing `consecutive_failures` reset without re-deriving whether the same reset
condition applies to both semantics.

**How to avoid:** Do not "fix" 18d by making the reset conditional in a way that also touches
`infra_failures`, unless the fix is proven not to break `transition_resets_infra_failures` (the
existing test) or narrow `MAX_INFRA_FAILURES`'s bound. The safer fix shape scopes the change to
`consecutive_failures` specifically — e.g., only reset it on a transition OUT of `Validate` when
the outcome was a genuine pass (not merely "any transition"), or move the reset to fire based on
`from`/`to` stage identity rather than unconditionally. There IS a latent, lower-priority
version of the same weakness in `infra_failures` (a systemically-flaky environment that causes
intermittent infra faults across MULTIPLE DIFFERENT stages, each of which occasionally succeeds
and resets the shared counter before the ceiling is reached) — but this is a materially
different trigger condition (multi-stage systemic flakiness vs. a single reliably-failing stage
pair) and is not proven to have occurred live, unlike 18d's Code↔Validate case (which the
CONTEXT.md documents as "observed live across three cycles"). Flag it in the plan as an
explicitly out-of-scope open question rather than silently fixing or silently ignoring it.

### Pitfall 3: 18e's "gate for a human on disagreement" needs a THIRD outcome, not a reused boolean

**What goes wrong:** `handle_validate_outcome(project_root, state, passed: bool)`
(`main.rs:1505`) and `advance()`'s Validate arm
(`let passed = matches!(result.verdict, Some(Verdict::Pass)); handle_validate_outcome(..,
passed)`, `main.rs:1361-1362`) are both built around a boolean. The operator's binding decision
requires THREE outcomes at Validate when `external_verify` is declared: (a) probe pass + verdict
pass → advance, (b) probe pass + verdict gaps → gate immediately (disagreement), (c) probe pass
+ no verdict at all → gate immediately (ambiguous). Only (a) is "true" in the existing boolean
model; naively mapping BOTH (b) and (c) to `passed = false` re-introduces exactly the auto-loop
behavior CONTEXT.md says must not happen for "ambiguous" outcomes — `handle_validate_outcome`'s
`false` branch auto-loops back to Code up to `MAX_CONSECUTIVE_FAILURES` times before gating,
which is a DELAYED gate, not the immediate gate the decision requires.

**Why it happens:** The existing cascade's `AgentResult` only carries a `Verdict` (Pass/Gaps) —
there's no way today to distinguish "the agent said Gaps" from "external evidence and the agent
disagree" from "no verdict arrived." All three currently collapse to the same `passed: false`
if handled naively.

**How to avoid:** Do not route the disagreement/no-verdict cases through
`handle_validate_outcome`'s counter-based auto-loop. Route them through
`handle_stage_failure`-shaped immediate gating instead (Pattern 3 above), OR add a distinct
reason/flag on `AgentResult` (e.g. a `Some("ambiguous: external verify passed but ...")`
`reason` string checked at the `advance()` dispatch site before calling
`handle_validate_outcome`) that forces the gate path regardless of `consecutive_failures`. Both
are viable; the planner should pick one explicitly rather than let it fall out of whichever
`passed` mapping is written first. This is Claude's/the planner's discretion — CONTEXT.md locks
the DECISION MATRIX (what triggers advance vs. gate), not the code shape that implements it.

### Pitfall 4: `enforce_build_staleness` still hard-blocks based on `project_root`'s OWN Cargo.toml identity even after 18c

**What goes wrong:** `is_self_dogfood_workspace(project_root)` (the check for "is this DevFlow's
own workspace") must stay anchored on `project_root`, not `execution_root` — a worktree checked
out from DevFlow's own repo still has DevFlow's `Cargo.toml` at its root (git worktrees share
the same tracked files at the commit they're checked out to), so this particular check is
probably safe to leave as-is or point at either root with an identical result. But it's worth an
explicit assertion in the fix's tests rather than an assumption, since a worktree's `Cargo.toml`
content is whatever commit the worktree is on — if a PLAN mid-flight modified `Cargo.toml`'s
`members` array on the feature branch (unlikely but not impossible), the two roots could
disagree.

**Why it happens:** 18c's fix threads `execution_root` through the ANCESTRY/dirty-tree checks
but the self-dogfood IDENTITY check answers a different question ("is this workspace DevFlow's
own repo at all", not "is the binary stale relative to X") and was not itself flagged as buggy
in CONTEXT.md.

**How to avoid:** Add a test with divergent `project_root`/`worktree_path` `Cargo.toml` content
(if feasible) or explicitly document in the plan why `is_self_dogfood_workspace` intentionally
stays project_root-scoped.

## Code Examples

### The exact defect site for 18d (verify against this before writing the fix)

```rust
// Source: crates/devflow-cli/src/main.rs:1788-1811 (current HEAD)
fn transition(project_root: &Path, state: &mut State, to: Stage) -> Result<(), CliError> {
    let from = state.stage;
    let _ = run_checkout_hooks(
        project_root,
        state,
        &hooks::hooks_for_transition(from, to),
        to,
    );
    state.stage = to;
    state.consecutive_failures = 0;  // <-- unconditional; BUG for the Code->Validate hop
    state.infra_failures = 0;        // <-- unconditional; correct per mode.rs's own doc (Pitfall 2)
    state.gate_pending = false;
    workflow::save_state(state)?;
    events::emit(/* "transition" event */);
    launch_stage(state, None, Some(from))
}

// The call site that crosses the boundary every retry cycle:
// crates/devflow-cli/src/main.rs:1351-1353 (advance()'s Code-stage success arm)
Stage::Code => transition(project_root, &mut state, Stage::Validate),
```

### The exact defect site for 18e (verify against this before writing the fix)

```rust
// Source: crates/devflow-core/src/agent_result.rs:782-794 (current HEAD)
match commands
    .into_iter()
    .find(|command| !crate::verify::run_external_verification(command, execution_root))
{
    Some(command) => Some(AgentResult { status: AgentStatus::Failed, /* .. */ verdict: None, decided_by_layer: Some(0) }),
    // Every declared, approved probe passed — affirmative completion
    // evidence on its own (D-05 gap 2), even with zero commits.
    None => Some(AgentResult {
        status: AgentStatus::Success,
        exit_code: None,
        reason: Some("external verification passed — all declared, approved probes succeeded".into()),
        commits: None,
        summary: None,
        verdict: None,          // <-- BUG at Validate: discards Layer 1's verdict entirely
        decided_by_layer: Some(0),
    }),
}

// The cascade never reaches Layer 1 once Layer 0 returns Some(..):
// crates/devflow-core/src/agent_result.rs:815-817
if let Some(result) = evaluate_layer0(project_root, state, approved_commands) {
    return Ok(result);   // Layer 1's DEVFLOW_RESULT marker (the only verdict carrier) never runs
}

// advance()'s consumer, which then computes passed = false:
// crates/devflow-cli/src/main.rs:1360-1362
let passed = matches!(result.verdict, Some(Verdict::Pass));
handle_validate_outcome(project_root, &mut state, passed)
```

### The exact defect site for 18f (verify against this before writing the fix)

```rust
// Source: crates/devflow-cli/src/main.rs:796-825 (current HEAD)
fn run_preflight(
    project_root: &Path,
    state: &mut State,
    adapter: &dyn agents::AgentAdapter,
) -> Result<bool, CliError> {
    let stage = state.stage;
    if let Err(reason) =
        generic_preflight_checks(project_root, state).and_then(|()| adapter.preflight(state))
    {
        let context = format!(/* "[never-silent] preflight failed ..." */);
        match run_gate(project_root, state, stage, &context)? {
            GateAction::Advance => {
                let _ = Gates::cleanup(project_root, state.phase, stage);
                state.gate_pending = false;
                launch_stage(state, None, None)?;   // <-- re-enters run_preflight, re-runs the SAME deterministic check
            }
            GateAction::LoopBack(_) => {
                let _ = Gates::cleanup(project_root, state.phase, stage);
                launch_stage(state, None, None)?;   // <-- same re-entry
            }
            GateAction::Abort(reason) => abort(project_root, state, &reason)?,
        }
        return Ok(false);
    }
    Ok(true)
}
```

### The existing pattern 18g should copy (already fixed for the sibling helper in the same file)

```rust
// Source: crates/devflow-cli/tests/phase7_cli.rs:91-99 (plain, racy — this IS
// the shape the flaky assertion at lines 199-200 currently uses) vs.
// phase7_cli.rs:101-114 (the ALREADY-FIXED retry-based sibling, written for
// the identical archive-timing race against the pid file):
fn wait_for(path: &Path) {              // racy one-shot check (what lines 199-200 use today)
    for _ in 0..200 {
        if path.exists() { return; }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for {}", path.display());
}

/// Wait until a monitor-written pid file exists AND holds a parseable pid,
/// returning it. A plain `wait_for` + one-shot read is racy: each stage
/// transition's `archive_phase_files` briefly deletes the pid file before
/// the next monitor recreates it, so a read can land in the gap and hit
/// NotFound even though the pipeline is healthy.
fn wait_for_pid(path: &Path) -> u32 { /* retry loop reading + parsing, not just exists() */ }
```

The fix for 18g's final assertions (`phase7_stdout.exists()`/`phase8_stdout.exists()` at
`phase7_cli.rs:199-200`) is to either (a) move those two assertions to run immediately after
their respective `wait_for()` calls at lines 186-187, before the unrelated `state7`/`state8`
assertions run, or (b) replace the final `.exists()` checks with a retry-tolerant helper mirror-
ing `wait_for_pid`'s "the file legitimately disappears and reappears during archival" contract.
Option (a) is the smaller, more surgical fix and directly addresses CONTEXT.md's stated fix:
"assert the capture immediately."

## State of the Art

Not applicable in the domain-drift sense (no external library/API has moved since this code was
written days ago). The relevant "state of the art" is entirely intra-repo: every fix in this
phase is downstream of Phase 17's own changes (17-03 introduced 18e's regression; 17-06
introduced 18d's `infra_failures` reset alongside the pre-existing `consecutive_failures` reset;
17-08 fixed a DIFFERENT double-launch bug in the same `run_preflight` function 18f now touches
— confirm the 18f fix does not reintroduce 17-08's regression, i.e. `run_preflight`'s `Ok(bool)`
return contract that lets the caller know whether it already fully handled the launch must be
preserved).

**Deprecated/outdated:** None — this is the current, only version of every touched function.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The recommended fix shape for 18d (scope the reset to `consecutive_failures` only, or make it conditional on transition identity, rather than blanket-removing it) is the correct engineering choice, not merely one option among several | Common Pitfall 2 | Low — this is presented as a recommendation with explicit reasoning grounded in `mode.rs`'s own doc comment and an existing passing test (`transition_resets_infra_failures`); the planner retains discretion to choose a different conditional-reset shape as long as it doesn't regress that test |
| A2 | 18e's disagreement/no-verdict cases should route through `handle_stage_failure`-shaped immediate gating rather than a new `AgentResult` reason-flag mechanism | Common Pitfall 3 | Medium — both are presented as viable; if the planner picks the reason-flag approach instead, the `advance()` dispatch-site changes described here don't directly apply, though the underlying three-way-outcome requirement is unaffected |
| A3 | `is_self_dogfood_workspace` should stay `project_root`-scoped rather than also switching to `execution_root` | Common Pitfall 4 / 18c | Low — flagged explicitly as an open question for the plan to either test or explicitly document, not asserted as settled |

## Open Questions

1. **Does 18d's fix need a new `mode.rs` predicate, or is a targeted change inside
   `transition()` sufficient?**
   - What we know: the reset currently fires unconditionally inside `transition()`
     (main.rs), while the ceiling constant and gate-decision logic live in `mode.rs`
     (devflow-core). `mode.rs`'s own doc comment already documents the reset's *intent* for
     `infra_failures`, suggesting the reset is meant to be understood as part of that module's
     contract even though the code lives elsewhere.
   - What's unclear: whether the fix should move reset logic into `mode.rs` (e.g. a
     `Mode::should_reset_consecutive_failures(from, to)` predicate, consistent with the
     existing `should_gate`/`should_auto_loop` predicates already there) for testability
     parity, or stay a `main.rs`-local conditional.
   - Recommendation: given `mode.rs` already owns `should_gate`/`should_auto_loop` as pure,
     directly-unit-testable predicates over `(stage, mode, consecutive_failures)`, adding a
     sibling pure predicate there is the more consistent shape and gives 18d the same
     directly-unit-testable quality `mode.rs`'s existing tests already have (see
     `mode.rs:98-157`'s test module) — but this is a real design choice, not a locked decision.

2. **18f's "bound the recursion regardless as a backstop" — what's the ceiling and where does
   it live?**
   - What we know: the operator decision explicitly requires a backstop bound independent of
     the Advance-skips/LoopBack-rechecks split, in case the LoopBack path (which keeps
     re-running the check) itself gets stuck in a human-approves-then-fails-again cycle.
   - What's unclear: whether this reuses `MAX_CONSECUTIVE_FAILURES`/`MAX_INFRA_FAILURES` (an
     existing constant) or needs its own (`MAX_PREFLIGHT_RETRIES`), and whether the counter
     lives on `State` (persisted, survives monitor restarts — consistent with
     `consecutive_failures`/`infra_failures`'s existing persistence pattern) or is purely
     recursion-depth-scoped (a function parameter, not persisted).
   - Recommendation: persist it on `State` following the existing `consecutive_failures`
     pattern (a new `preflight_retries: u32` field, `#[serde(default)]`), since the wedge
     CONTEXT.md documents happened across separate `devflow` invocations (the monitor died and
     was manually recovered), which a purely in-recursion counter would not survive.

## Environment Availability

Skipped — this phase is pure Rust code + test changes against an already-checked-out, already-
building workspace. No new external tool, service, or runtime dependency is introduced by any
of 18a–18g. `git`, `cargo`, and `gh` (used by the EXISTING `preflight_gh_auth_check`, untouched
by this phase's scope) are already required by the current codebase and were confirmed present
and functioning during this research session (`cargo test --workspace`, `cargo clippy`, `cargo
fmt --check` all ran successfully).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in Rust harness); inline `#[cfg(test)]` modules in `main.rs`/`agent_result.rs`/`state.rs`/`mode.rs`, plus integration tests in `crates/devflow-cli/tests/` |
| Config file | none — workspace `Cargo.toml`; CI parity via `.github/workflows/ci.yml` |
| Quick run command | `cargo test -p devflow-cli <module>::` or `cargo test -p devflow-core <module>::` scoped to the touched module |
| Full suite command | `cargo test --workspace` (confirmed green at HEAD: 380 passed, 0 failed, ~35s wall) |
| CI-parity quality gates | `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` (both confirmed exit 0 at HEAD) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| 18a | `doctor` reports a diff between `State.stage`, the branch's actual commits, the latest event, and open gates for a phase with deliberately mismatched fixtures | unit | `cargo test -p devflow-cli doctor` | ❌ Wave 0 — new test module needed around the reconciliation logic |
| 18a | `doctor` mutates nothing by default (read-only contract) | unit | assert no `State`/event-log writes occur during a `doctor` call that finds a mismatch | ❌ Wave 0 |
| 18b | `monitor_pid` round-trips through `State` serde (mirrors `infra_failures_round_trips_through_serde`) | unit | `cargo test -p devflow-core state::tests::monitor_pid` | ❌ Wave 0 |
| 18b | `status`/`doctor` render the third row of the liveness matrix (monitor dead, agent dead → "stuck") distinctly from (monitor alive, agent dead → normal between-stages) | unit | `cargo test -p devflow-cli status_renders_dead_monitor` (new) | ❌ Wave 0 |
| 18c | `embedded_commit_is_stale`/`combined_staleness` evaluate against `state.worktree_path`'s HEAD, not `project_root`'s, when a worktree is set — RED: construct a fixture where `project_root` is `Fresh` but the worktree is two commits behind, assert current code wrongly reports `Fresh`/`Ahead`, then assert fixed code reports `Stale` | unit | `cargo test -p devflow-cli embedded_commit_is_stale_uses_worktree_head` (new) | ❌ Wave 0 — needs a git worktree test fixture (`git worktree add`), which no existing staleness test constructs (all 8 current staleness tests operate on `project_root` alone — confirmed via grep) |
| 18c | Self-dogfood binary behind a worktree HEAD is BLOCKed, not warned | unit | extend `enforce_build_staleness_blocks_self_dogfood_and_records_event_before_erroring`-style test with `state.worktree_path` set | ❌ Wave 0 |
| 18d | `consecutive_failures` reaches `MAX_CONSECUTIVE_FAILURES` across N repeated Code-succeeds/Validate-fails cycles — RED first (loop `handle_validate_outcome(false)` + `transition(.., Validate)` N times, assert counter is currently always 0 or 1, never reaching 3), then GREEN after the fix | unit | `cargo test -p devflow-cli consecutive_failures_reaches_ceiling_across_cycles` (new) | ❌ Wave 0 |
| 18d | `transition_resets_infra_failures` (existing test) still passes unchanged after the fix — regression guard proving 18d did not widen `infra_failures`' reset scope | unit (existing) | `cargo test -p devflow-cli transition_resets_infra_failures` | ✅ exists |
| 18e | Layer 0 affirmative-success at `Stage::Validate` with Layer 1 verdict `Pass` → `advance()` computes `passed = true` — RED: current code always computes `false` | unit + integration | `cargo test -p devflow-core layer0_affirmative_success_consults_layer1_verdict_at_validate` (new) | ❌ Wave 0 |
| 18e | Layer 0 pass + Layer 1 verdict `Gaps` (disagreement) → immediate gate, not the auto-loop path | unit | `cargo test -p devflow-cli external_verify_disagreement_gates_immediately` (new) | ❌ Wave 0 |
| 18e | Layer 0 pass + no verdict at all (ambiguous) → immediate gate | unit | `cargo test -p devflow-cli external_verify_no_verdict_gates_immediately` (new) | ❌ Wave 0 |
| 18e | Existing cascade tests (`layer0_affirmative_success_on_non_code_stage_with_zero_commits`, `layer0_affirmative_success_outranks_layer1_failure_marker`) still pass — neither currently asserts `verdict`, both must be extended to pin it post-fix | unit (existing, extend) | `cargo test -p devflow-core layer0_affirmative_success` | ✅ exists, needs extension |
| 18f | `GateAction::Advance` on a preflight gate does NOT re-run the failing check — RED: replace `FailOnceAdapter` with an `AlwaysFailAdapter` (unconditionally fails `preflight()`), seed exactly one gate response, assert current code either hangs on `poll_response` or (bounded via test's `DEVFLOW_GATE_TIMEOUT_SECS=2` override, matching the existing `ENV_MUTEX`-guarded pattern at `main.rs:4011-4019`) returns a second-gate timeout error; GREEN after the fix: agent launches exactly once, no second gate is written | integration | `cargo test -p devflow-cli run_preflight_advance_skips_recheck_on_idempotently_failing_check` (new) | ❌ Wave 0 — new `AlwaysFailAdapter` fixture required; CONTEXT.md explicitly flags `FailOnceAdapter` as unable to reproduce this |
| 18f | `GateAction::LoopBack` still re-runs the check (unchanged behavior) but the recursion is bounded — RED: `AlwaysFailAdapter` + repeated `LoopBack` responses, assert current code either recurses unboundedly (stack/time) or wedges; GREEN: bounded abort after N retries | integration | `cargo test -p devflow-cli run_preflight_loopback_bounds_recursion` (new) | ❌ Wave 0 |
| 18g | `parallel_creates_two_worktrees_and_spawns_two_monitors` passes reliably under repeated runs (RED: run existing test 25× in a loop before the fix if it can be shown to flake locally within a reasonable window; if not locally reproducible, treat the fix as prevention-only and rely on the reasoning already documented at `phase7_cli.rs:101-105`) | integration (existing, fix) | `for i in $(seq 1 25); do cargo test -p devflow-cli --test phase7_cli parallel_creates_two_worktrees_and_spawns_two_monitors -- --exact || break; done` | ✅ exists, modify assertion placement only |

### Sampling Rate
- **Per task commit:** scoped `cargo test -p devflow-cli <module>::` or `-p devflow-core
  <module>::` for the touched module, plus `cargo clippy --workspace --all-targets -- -D
  warnings` (CI-parity form, not the narrower `cargo clippy -- -D warnings` — WR-08 already
  found the narrow form misses `#[cfg(test)]`-only warnings).
- **Per wave merge:** full `cargo test --workspace` (baseline: 380 passed / 0 failed at
  HEAD — any new failure not explained by an in-progress RED test is a regression).
- **Phase gate:** full suite green, clippy clean, `cargo fmt --check` clean before
  `/gsd-verify-work`. Additionally, per WR-07's still-open finding: `build_provenance.rs`
  (3 tests, ~27s locally, confirmed in this session) is the one flaky-under-contention test in
  the suite — if a wave's CI run shows it fail, re-run in isolation
  (`cargo test -p devflow --test build_provenance`) before treating it as a real regression
  from this phase's changes.

### Wave 0 Gaps
- [ ] `crates/devflow-cli/src/main.rs` test module — `doctor` reconciliation tests (18a): no
  existing test drives `doctor()` against a deliberately-mismatched fixture (state says Code,
  branch has no Code-stage commits, an open gate exists nobody's seen). All of `doctor`'s
  current tests are environment/tool-check tests only.
- [ ] `crates/devflow-core/src/state.rs` — `monitor_pid` field + its serde round-trip test
  (18b), following the exact shape of `infra_failures_round_trips_through_serde` /
  `infra_failures_absent_from_json_defaults_to_zero`.
- [ ] A git-worktree test fixture helper (18c) — no existing staleness test constructs an
  actual `git worktree add`; one is needed to assert ancestry against a worktree HEAD distinct
  from `project_root`'s HEAD. `crates/devflow-core/src/worktree.rs`'s own test module already
  constructs worktrees for other purposes (`worktree::tests::add_creates_worktree_on_new_branch`)
  and is the closest existing precedent for the fixture shape.
- [ ] `AlwaysFailAdapter` test fixture (18f) — `crates/devflow-cli/src/main.rs`'s test module
  has `FailOnceAdapter` (fails once) and (implicitly, referenced but not shown in this excerpt)
  an "AlwaysRejectAdapter"-style adapter for other tests; confirm during planning whether an
  unconditionally-failing `preflight()` adapter already exists elsewhere in the test module
  before writing a new one (a targeted grep for `AlwaysReject` during Wave 0 setup is
  recommended — this research pass did not exhaustively enumerate every test fixture in the
  ~1,300-line test module).
- [ ] Framework install: none — `cargo test` is already fully configured and green.

## Security Domain

`security_enforcement` is not explicitly set to `false` in `.planning/config.json` — treating as
enabled per the protocol default. This phase is internal reliability/state-machine hardening on
a local CLI tool with no network-facing surface, no new external input parsing, and no new
authentication/session/crypto code. 17-REVIEW.md's Round 5 "Verified clean" section already
confirms this codebase has zero HTTP clients, no secrets in the dependency tree, and
argv-array `Command` usage throughout (no shell injection surface) — none of that changes in
this phase.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V1 Architecture, Design and Threat Modeling | Partial | The Code↔Validate/preflight unbounded-loop defects (18d, 18e, 18f) are availability/DoS-adjacent failure modes (a local resource-exhaustion-via-infinite-retry class, not an attacker-facing DoS) — the fix pattern (bounded counters, fail-closed-to-a-human-gate) is the standard mitigation already used elsewhere in this codebase (`MAX_INFRA_FAILURES`, the `[never-silent]` gate idiom) and should be followed, not reinvented |
| V2 Authentication | No | No auth surface touched |
| V3 Session Management | No | N/A — `State`/`.devflow/` is local process state, not a session/auth boundary |
| V4 Access Control | No | Single-operator local CLI; no multi-tenant or privilege boundary |
| V5 Input Validation | No | No new external input parsing introduced; `git`/`gh` subprocess invocations remain argv-array (existing pattern, unchanged) |
| V6 Cryptography | No | Not touched |
| V7 Error Handling and Logging | Yes | The `[never-silent]` gate idiom (`handle_stage_failure`, and 18f's fix must preserve it) is this codebase's standing V7-equivalent control: every failure path either advances, gates visibly, or aborts with a logged event — never a silent hang. 18f's fix must not introduce a new silent-hang path while fixing the old one; 18d/18e's fixes must preserve the existing `events::emit` calls on every branch. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Unbounded retry loop masquerading as healthy progress (18d/18e's Code↔Validate loop, 18f's preflight re-run) | Denial of Service (local resource/time exhaustion, not attacker-triggered) | Bounded counters persisted to `State` (existing `MAX_CONSECUTIVE_FAILURES`/`MAX_INFRA_FAILURES` pattern) that force a human gate or abort once a ceiling is reached — this phase's fixes must make the EXISTING mitigation actually reachable, not invent a new one |
| Silent process death with no operator signal (18b's monitor liveness gap) | Denial of Service / availability | Liveness probing + explicit state representation (the three-row matrix in CONTEXT.md/OPERATOR-OBSERVABILITY-FINDINGS.md Finding 1) rather than an ambiguous "may have already advanced" message |
| Stale binary silently re-running pre-fix logic against live state (18c) | Tampering (of evidence/provenance, not of an attacker-controlled input) — a stale binary can silently reintroduce an already-fixed defect and misreport success | `enforce_build_staleness`'s existing hard-block-on-self-dogfood pattern, corrected to evaluate the tree that's actually driving the binary (the worktree) rather than the tree that happens to share a filesystem path prefix |

## Sources

### Primary (HIGH confidence — direct source inspection this session)
- `crates/devflow-cli/src/main.rs` (6239 lines) — `doctor` (18a), staleness functions (18c),
  `transition`/`handle_validate_outcome`/`handle_infra_outcome` (18d), `advance()` dispatch
  (18e consumer), `run_preflight` (18f), all cited line ranges verified live against HEAD
- `crates/devflow-core/src/agent_result.rs` (2397 lines) — `evaluate_layer0`/
  `evaluate_agent_result_inner`/`evaluate_layer1` (18e)
- `crates/devflow-core/src/state.rs` (233 lines) — `State` struct, confirmed no `monitor_pid`
  field (18b)
- `crates/devflow-core/src/mode.rs` (157 lines) — `MAX_CONSECUTIVE_FAILURES`,
  `MAX_INFRA_FAILURES`, `should_gate`, doc comments (18d)
- `crates/devflow-core/src/monitor.rs` (240 lines) — `spawn_monitor_inner` (18b spawn-time
  persistence hook)
- `crates/devflow-core/src/recover.rs` (233 lines) — existing liveness/reconciliation
  primitives to reuse for 18a
- `crates/devflow-cli/tests/phase7_cli.rs` (642 lines) — `parallel_creates_two_worktrees_and_
  spawns_two_monitors` (18g), `wait_for`/`wait_for_pid` (the already-fixed sibling pattern)
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --check` — all run live this session, confirmed green baseline (380/0/0)

### Secondary (MEDIUM confidence — project documentation, cross-checked against source)
- `.planning/phases/18-dogfood-reliability-hardening/CONTEXT.md` — phase scope, both binding
  operator decisions (verbatim, cross-checked against source and found accurate on every cited
  line/behavior)
- `.planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md` — CR-01 (=18e), CR-02 (=18f),
  WR-11 (mode.rs stale doc claim, relevant to 18d), all findings cross-checked against current
  source and confirmed still present
- `.planning/OPERATOR-OBSERVABILITY-FINDINGS.md` — Finding 1 (=18b), Finding 2 (partially
  duplicate of 18b, rest deferred to backlog `999.2`/`999.3`)
- `.planning/ROADMAP.md` (Phase 18 + Phase 17 sections), `.planning/STATE.md` (2026-07-20
  decision entry) — sequencing/history context

### Tertiary (LOW confidence)
- None used — this phase required no external web research; every claim traces to a file this
  session read directly.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies, confirmed by direct `Cargo.toml` inspection
- Architecture: HIGH — every fix site read and quoted directly from current HEAD, cross-checked
  against 17-REVIEW.md's prior findings and found to still match
- Pitfalls: HIGH — Pitfalls 1-4 derived from direct code tracing (not speculation), including
  the nuanced `infra_failures`-vs-`consecutive_failures` asymmetry (Pitfall 2), which required
  tracing four call paths (`handle_infra_outcome`, `handle_rate_limited_outcome`,
  `handle_stage_failure`, `transition`) to confirm

**Research date:** 2026-07-20
**Valid until:** Line numbers and exact function shapes are valid until the next commit touches
`main.rs`/`agent_result.rs` — given this phase's own plans will touch exactly these files,
re-verify line numbers (not logic) immediately before each task starts, not just once at
research time. Logic/architecture findings remain valid for the life of this phase (no external
dependency drift risk).
