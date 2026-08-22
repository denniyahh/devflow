---
phase: 18-dogfood-reliability-hardening
reviewed: 2026-07-21T02:00:00Z
depth: deep
files_reviewed: 6
files_reviewed_list:
  - crates/devflow-cli/src/main.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-cli/tests/phase7_cli.rs
  - crates/devflow-cli/tests/log_format_env.rs
findings:
  critical: 0
  warning: 4
  info: 0
  total: 4
status: issues_found
---

# Phase 18: Code Review Report

**Reviewed:** 2026-07-21T02:00:00Z
**Depth:** deep (cross-file invariant tracing, plus a live repro of one finding against the built binary)
**Files Reviewed:** 6
**Status:** issues_found

## Summary

Reviewed the net diff `8d67d1e..8f7cabd` (commits `18-01`..`18-07`) against `crates/`. `cargo build --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` are all clean, confirming the stated gate status.

This is a state-machine bug-fix phase, so I traced the five things flagged for extra scrutiny end-to-end rather than trusting the doc comments:

1. **`unreachable!()` in `handle_validate_outcome` (main.rs:1752).** Traced every path into the trailing `match outcome { .. }`: it is reached only when the preceding `if forced || should_gate { return ...; }` block did NOT return, which is only possible when `forced` was `false` at evaluation time — and `forced` is `matches!(outcome, Ambiguous(_))` computed from the same, unmutated `outcome` binding a few lines earlier. No path exists to reach the `Ambiguous` arm with a live `Ambiguous` value. **The invariant is airtight as written.** See WR-03 below for a quality concern about *how* it's enforced, not whether it holds.
2. **18d/18e transition-reset scoping.** `transition_resets_consecutive_failures(from, to)` returns `false` only for `(Code, Validate)`. I enumerated every real (non-test) call site of `transition()` — `Define→Plan`, `Plan→Code`, `Code→Validate` (the one exclusion), `Validate→Ship` (×2) — and every retry/loop-back path (`handle_stage_failure`, `loop_back_to_code`) that could otherwise cross `(Code, Validate)` or `(Validate, Code)`: none of them call `transition()` — `loop_back_to_code` mutates `state.stage` directly and bypasses it entirely. **The fix is exactly scoped; no other transition's reset behavior changed.** `state.infra_failures = 0;` remains unconditional in `transition()` (main.rs:2023), unaffected by the new guard.
3. **`preflight_retries` reset discipline.** One increment site (main.rs:844, `saturating_add`), two reset sites: preflight-pass (main.rs:879-881, persisted, guarded to avoid a no-op write) and `GateAction::Advance` (main.rs:859, persisted before the bypassed relaunch). The retry-ceiling abort path (main.rs:825-861) calls `abort()`, which clears the whole state file, so no code path leaves a non-zero counter behind for a *future* run. Serde back-compat verified for both `preflight_retries` and `monitor_pid`: both are `#[serde(default)]` and both have a direct unit test deserializing a JSON blob that omits the field. **Sound.**
4. **`saturating_add` coverage.** Grepped every mutation site of `consecutive_failures`, `infra_failures`, and `preflight_retries`: all three exclusively use `.saturating_add(1)` (main.rs:844, 1555, 1601, 1715). No plain `+= 1` remains on any of these three persisted counters. **Sound.**
5. **WR-02 leak class (home path / username into a persisted artifact).** This is where I found something: the self-dogfood staleness-block message — which `18-05`/`18c` changed to report `execution_root` instead of `project_root` — is still written into `.devflow/events.jsonl`'s `self_dogfood_stale_blocked` event (main.rs:1204-1213, pre-existing from 17d, unchanged by this phase's edit to *which* path is embedded). This directly contradicts the "stays terminal-only" expectation and is the same leak class the phase's own `18-03-PLAN.md` threat table (T-18-09) explicitly calls out and guards against for the *new* monitor/doctor output. See WR-02 below — flagged as WARNING, not CRITICAL, consistent with how `17-REVIEW.md` classified the same class as WR-02 there.

Beyond the five requested items, cross-file tracing of the new `doctor` reconciliation (18a) and monitor-liveness (18b) machinery surfaced two further defects: a reproducible `--json` output-contract break in `doctor`, and a narrow but real false-positive in the new "Stuck" liveness classification caused by an interaction between 18b (persisted `monitor_pid`) and pre-existing/18c-widened failure paths in `launch_stage_inner`. Both below as WR-01 and WR-04.

No Critical-severity findings. All four Warnings are real, reproducible-or-proven defects — not style nitpicks.

## Warnings

### WR-01: `devflow doctor --json` emits two concatenated JSON documents, not one

**File:** `crates/devflow-cli/src/main.rs:3442-3443` (call site), `render_reconciliation_json` (~3926-3959)

`doctor()`'s pre-existing tool-checks section prints a complete, self-terminating JSON array (`print!("{out}")`, ending in `]\n`) when `json` is true. The new line added by this phase —

```rust
let facts = collect_phase_facts(project_root);
render_reconciliation(&facts, json);
```

— then prints a **second**, independent top-level JSON array immediately after it (`render_reconciliation_json` also starts with `"[\n"` and ends with `"]\n"`). I built the binary and reproduced this directly against a live fixture with one active phase:

```
$ devflow doctor --json > out.json
$ python3 -c "import json; json.load(open('out.json'))"
json.decoder.JSONDecodeError: Extra data: line 57 column 1 (char 1073)
```

`jq '.'` tolerates this (it streams multiple top-level values), but any consumer using a standard single-document parser (`json.load`, `JSON.parse`, most language stdlib JSON readers) will fail on the full `--json` output whenever at least one active phase exists — which is exactly the situation `doctor`'s reconciliation feature exists to help diagnose. `18-01-PLAN.md` documents this as an intentional design choice ("emit a second [...]"), so it isn't an accident, but it still breaks the ordinary expectation that a `--json` flag produces one JSON document.

**Fix:** either merge both arrays into a single JSON object (`{"checks": [...], "reconciliation": [...]}`), or clearly document that `--json` output is NDJSON-style (multiple concatenated top-level values) and adjust `OPERATIONS.md`'s description of the flag accordingly so downstream consumers know to use a streaming parser.

### WR-02: self-dogfood staleness-block message still writes an absolute filesystem path into `.devflow/events.jsonl`

**File:** `crates/devflow-cli/src/main.rs:1188-1213` (`enforce_build_staleness`)

```rust
let message = format!(
    "self-dogfood stale build blocked for stage {}: ... ancestor of \
     {}'s current HEAD ...",
    state.stage,
    execution_root.display(),   // 18c: was project_root.display() before this phase
    ...
);
gates::fire_gate_notify(state.phase, state.stage, &message, true);
events::emit(
    project_root,
    state.phase,
    "self_dogfood_stale_blocked",
    serde_json::json!({
        "stage": state.stage.to_string(),
        "reason": truncate_reason(&message),   // <-- persisted, path and all
    }),
);
Err(CliError::Message(message))
```

`truncate_reason` caps at 300 chars but only strips control characters — it does not redact paths, and the path substring sits well within the first ~150 characters of `message`, so it survives truncation intact. `execution_root` is `project_root` (unchanged case) or the phase's worktree path (18c's new case), both of which live under the operator's home directory in the common case (this very repo: `/var/home/<user>/Github/devflow`). This is the exact WR-02 leak class from `17-REVIEW.md` ("`exe_path` ... writes the developer's absolute home path and OS username into `.devflow/events.jsonl`") — the same document `18-03-PLAN.md`'s own threat table (T-18-09) cites by name when justifying why the *new* monitor-liveness output must never carry a path.

This is not a regression introduced fresh by this phase — `events::emit`'s `"reason": truncate_reason(&message)` line predates Phase 18 (17d). Phase 18c's contribution was swapping which root gets embedded (`project_root` → `execution_root`) and, for worktree phases, appending an extra path-adjacent sentence — it neither introduced nor fixed the leak. Flagging it here because (a) this phase's own task explicitly asked to confirm the message "stays terminal-only," which it does not, and (b) 18-03 demonstrates the team was already alert to exactly this class two plans later in the same phase, making it a live, actionable gap rather than accepted debt.

**Fix:** emit a path-free `reason` for this event — e.g. `state.stage` plus a boolean `worktree: bool` and the bare `Staleness` variant name, with the full path reserved for the `Err(CliError::Message(message))` that only ever reaches the terminal/notify command, never `events.jsonl`.

### WR-03: the trailing `unreachable!()` in `handle_validate_outcome` is sound today but enforced only by control flow, not the type system

**File:** `crates/devflow-cli/src/main.rs:1719-1755`

Confirmed (see Summary point 1) that the arm cannot currently be reached. That said, two things make this worth flagging rather than accepting quietly:

- It's the only `unreachable!()`/panic-capable macro anywhere in `main.rs`'s production code (`rg -n "unreachable!|panic!"` outside `#[cfg(test)]` returns exactly this one hit). Every other error condition in this file — including several documented as far more likely to occur — routes through `Result<_, CliError>` and the "never-silent" gate/abort machinery. A hit on this line is a bare `panic!`, which in a detached-monitor process is a genuinely silent failure mode (no gate, no notify, just a crashed process) — precisely the class of incident this whole phase exists to eliminate.
- The proof of unreachability spans two disconnected blocks ~30 lines apart (the `forced` computation feeding the early-return `if`, and the trailing `match`), connected only by the reader's own manual reasoning, not by the compiler. A future edit to either block in isolation — e.g., reordering the `if forced || should_gate` short-circuit, or adding a new early return between the two — can silently reintroduce reachability. Existing tests (`handle_validate_outcome` is exercised with `Ambiguous` outcomes at main.rs:5230, 5274, 5660) would likely catch a regression via a hard panic during `cargo test`, which is some protection, but a panicking test failure is a much worse signal than "code doesn't compile."

**Fix:** restructure so the compiler enforces the invariant instead of a comment: e.g., match on `outcome` first and return immediately for the `Ambiguous` case (building its gate context and calling `run_gate` inline) before the `if forced || should_gate` block, leaving only `Passed`/`Failed` for the final two-armed match — eliminating the third arm (and the `unreachable!()`) entirely rather than proving it dead.

### WR-04: a launch failure between a stage transition and monitor spawn leaves a stale `monitor_pid`, which the new liveness check then misreports as `Stuck`

**File:** `crates/devflow-cli/src/main.rs:1242-1324` (`launch_stage_inner`), `2769-2905` (`Liveness`, `status`), `check_dead_monitor` (~2844 area)

`state.monitor_pid` is only overwritten *after* `monitor::spawn_monitor` succeeds (main.rs:1301-1307). Every step before that in `launch_stage_inner` can fail and return early via `?`: `ensure_agent_binary` (1261), and notably `enforce_build_staleness` (1268-1273) — which 18c made *more* likely to correctly fire for worktree-based phases (that was the point of 18c). `transition()` itself already advanced and persisted `state.stage` to the new stage *before* calling into `launch_stage`/`launch_stage_inner` (main.rs:2011-2024), so on any of these early failures the persisted state ends up with:

- `state.stage` = the **new** stage (already committed),
- `state.monitor_pid` = the **previous** stage's monitor pid (now normally already exited, since that monitor's own exit is what triggered `devflow advance` in the first place) — or `None` if this is the phase's very first launch.

`doctor`/`status`'s new liveness classification (`liveness()`, main.rs:2801-2807) then reads exactly this stale value: dead monitor pid + a stage that legitimately has no live monitor yet → `Liveness::Stuck`, printing `"stuck — needs devflow resume"`. But `devflow resume` does nothing for a build-staleness block — the actual required action (rebuild the `devflow` binary) was already reported once, loudly, via the `CliError` that the original `devflow advance` invocation exited with. An operator who only checks `doctor`/`status` later (the exact workflow this phase's own dogfooding narrative describes) will be pointed at the wrong remedy.

**Fix:** either clear `state.monitor_pid` to `None` (and persist it) at the top of `launch_stage_inner`, before any fallible step, so a failed relaunch never carries forward a stale pid from a completed prior stage; or have `enforce_build_staleness`'s error path itself null out and persist `monitor_pid` before returning `Err`.

---

_Reviewed: 2026-07-21T02:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
