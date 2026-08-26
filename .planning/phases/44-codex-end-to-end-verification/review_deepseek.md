I've read all four plans, the CONTEXT/RESEARCH/PATTERNS/VALIDATION artifacts, and verified every cited source surface against the live tree (ship.rs, state.rs, preflight.rs, pipeline_launch.rs, pipeline_gate.rs, commands.rs, main.rs, recover.rs, codex.rs, hermes.rs, events.rs, plus the installed Hermes source `cron/jobs.py` and a live `hermes cron create --help`). Findings below are ordered by impact.

---

# Adversarial Plan/Architecture Review — Phase 44 (Codex E2E Verification & Driver Handoff Hardening)

## Summary of what I found

Most of the plan's **line-number citations are accurate** — `resume()` (pipeline_launch.rs:1237), `spawn_agent_and_record` (:969), `preflight_interactivity_check` (preflight.rs:607-634), `delete_cron_instructions` (ship.rs:139), `cron_instructions_path` (:67), `cron_schedule_from_retry_after` (:195), `RetryTimestamp::to_cron` (:218), `Command::Resume` (main.rs:167), the three hardcoded `--from-devflow` tests (commands.rs:4140-4198), `finish_workflow_with_gate_timeout` (pipeline_gate.rs:228), and the two `recover.rs` call sites (:117, :140) all check out. The Hermes contract claims are also substantively **correct**: `parse_schedule` (jobs.py:732-829) does keep an offset-qualified timestamp as-is via the `dt.tzinfo is None` gate at :800, and `--from-devflow` is genuinely absent from `hermes cron create --help`. The D-06 field-preservation list is complete and matches the actual `State` struct.

The defects are in **what the plans don't specify**, not in what they got wrong. Three are material: (1) an unspecified empty-schedule rendering branch that re-creates the #148 defect class, (2) a handoff pre-check that overstates its own safety guarantee, and (3) a set of undocumented call-site/audit-integrity gaps.

---

## HIGH

### H1 — `cron_hint_line` rewrite has no empty-schedule branch, and the empty-schedule record is demonstrably reachable

- **Severity:** High (broken operator-facing contract; re-creates #148's defect class at a new address)
- **Location:**
  - `44-03-PLAN.md` Task 2 `<behavior>` (lines 239-248): *"For a record with a resolved schedule, `cron_hint_line` renders a single line … whose positional schedule is the record's `hermes_cron.schedule`…"* — the empty case is never specified.
  - `crates/devflow-cli/src/pipeline_outcomes.rs:104-126` — the record is **written before** the empty-schedule check:
    ```rust
    let instructions = devflow_core::ship::build_single_agent_cron_instructions(project_root, phase, &retry_after);
    devflow_core::ship::write_cron_instructions(project_root, &instructions)?;   // line 106 — unconditional
    ...
    if instructions.hermes_cron.schedule.is_empty() {                            // line 117
        return gate_or_abort_infra(... "auto-resume cron not scheduled; resume manually");
    }
    ```
  - `crates/devflow-core/src/ship.rs:637-643` — `cron_instructions_reject_unparseable_retry_time` confirms `schedule.is_empty()` is the *expected* persisted state for unparseable input.
- **Defect:** `handle_rate_limited_outcome` persists a `CronInstructions` record with an **empty `hermes_cron.schedule`** (the unparseable-retry case, e.g. the `"usage limit"` 429 fallback), *then* routes to a gate. That empty-schedule record stays on disk and is listed by `list_cron_instructions` → `cron_instruction_hints` → `cron_hint_line`. 44-03 rewrites `cron_hint_line` to render `hermes cron create <schedule> <command> …` but only defines the "resolved schedule" shape. D-13 ("keep unparseable retry times fail-closed") is scoped to the schedule *value* only, not to the hint *rendering*.
- **Consequence & Reproduction:** Run a phase to a 429 with no `Retry-After` header. `devflow status` then prints a hint line with an **empty schedule positional** — `hermes cron create  'cd … && devflow resume --phase 7' --repeat 1 --name devflow-phase-07-resume (rate-limit resets: usage limit)`. The operator pastes it; Hermes either rejects it (empty schedule) or — worse — misparses `'cd …'` as the schedule and the `--name`/`--repeat` flags as free text. This is exactly the "plausible-looking but broken instruction" that issue #148 exists to eliminate, and 44-03's own P-04 prohibition ("replacing one unrun string with another unrun string reproduces the defect at a new address") would catch the *resolved* case but has no counterpart for this reachable empty case.

---

## MEDIUM

### M1 — `resume()` signature change breaks three existing test call sites the plan never mentions

- **Severity:** Medium (build-breaking omission; discoverable only at compile time)
- **Location:** `crates/devflow-cli/src/pipeline_launch.rs:1795, 1899, 1980` — `let result = resume(root, phase, false);` (three occurrences). Versus `44-01-PLAN.md` acceptance criteria (lines 227-230), which verify only `fn resume\(` arity and the main.rs dispatch.
- **Defect:** Adding `agent: Option<AgentKind>` as the third parameter makes all three existing calls `resume(root, phase, false)` fail to compile. 44-01's `files_modified`/`<action>`/acceptance criteria account for `main.rs:593` and the new tests, but never list updating these three pre-existing test call sites (they belong to `launch_stage_persists_monitor_pid_for_reload` and its siblings at ~1766-2006).
- **Consequence & Reproduction:** First `cargo test` after the change fails to compile the `devflow` test target. The plan's own `<verify>` (`cargo test -p devflow --bin devflow …`) surfaces it, but the executor discovers a broken build that is not in the plan's change list — a sequencing/waste issue rather than a silent failure.

### M2 — D-07's `agent_handoff` event overstates reality when the post-persistence relaunch fails

- **Severity:** Medium (repudiation/audit-integrity gap)
- **Location:** `44-01-PLAN.md` `must_haves` (line 26): *"…records an `agent_handoff` entry … before the relaunch spawns (D-07)."* and Task 1 `<action>` step 4 (lines 170-176). Compare `44-02-PLAN.md` D-15 negative control (lines 22, 98-104), which preserves the **record** on failed relaunch but adds no corrective **event**.
- **Defect:** The handoff event is emitted before `launch_stage` runs. If `spawn_monitor` (pipeline_launch.rs:1024) or `ensure_agent_binary` (:988) subsequently fails, the state on disk reads `agent=codex` with a `handoff` event already written, but no monitor was ever spawned. The audit trail records a handoff that never launched, with no `handoff_failed`/`stage_launch_failed` event to disambiguate it from a successful handoff.
- **Consequence & Reproduction:** `resume --agent codex` on a host where `codex` is not on PATH: interactivity pre-check passes (Codex is `HeadlessSafe` at Code/Validate/Ship), state mutates, event emits, then `ensure_agent_binary("codex")` errors. The event stream shows a clean handoff; a later reader reconstructing "what happened" cannot tell the launch never occurred. (The cron record correctly survives — D-15 is satisfied — but the *event* does not.)

### M3 — Handoff pre-check is narrower than "the same preflight seriousness as start" (D-08)

- **Severity:** Medium (overstated invariant; false audit event on driver-absent handoff)
- **Location:** `44-CONTEXT.md` D-08 (lines 62-65): *"Refuse unsafe handoffs with the same preflight seriousness as start."* vs. `44-01-PLAN.md` Task 1 `<action>` (lines 157-168), which reuses **only** `preflight_interactivity_check`. Compare `run_preflight` at `crates/devflow-cli/src/preflight.rs:1279-1286`:
  ```rust
  generic_preflight_checks(project_root, state).and_then(|()| driver.health(state))
  ```
  and `crates/devflow-core/src/agents/hermes.rs:56-68` (`health` = `hermes --version` presence probe; the default trait `health` in `agents/mod.rs:97-99` is a no-op).
- **Defect:** A normal `start` preflights `driver.health(state)` (e.g. Hermes's binary-presence check) in addition to the interactivity check. The handoff pre-check omits it. A `resume --agent hermes` on a host without `hermes` passes the interactivity gate, persists `state.agent=hermes`, emits `agent_handoff`, and only then fails — inside `run_preflight`'s `driver.health` gate (or `ensure_agent_binary`). The plan's D-08 language ("fail before state mutation") is only satisfied for the *interactivity* dimension, not the *driver-availability* dimension.
- **Consequence & Reproduction:** Half-handed-off state (`agent=hermes`, no monitor, no cron deletion) plus a false `agent_handoff` event. Recoverable via `resume --agent claude`, but the plan presents the pre-check as equivalent to start's preflight when it is a strict subset.

### M4 — `path_kind: "per-phase"` is a hardcoded misstatement, and a legacy-only record is never consumed on resume

- **Severity:** Medium (audit-trail inaccuracy + a real deletion gap)
- **Location:** `44-01-PLAN.md` Task 1 `<action>` (lines 183-194): pre-existence gate is `cron_instructions_path(...).exists()`, event hardcodes `path_kind: "per-phase"`. Versus `crates/devflow-core/src/ship.rs:139-154`:
  ```rust
  pub fn delete_cron_instructions(project_root, phase) -> Result<(), ShipError> {
      ... remove per-phase file ...
      let legacy = legacy_cron_instructions_path(project_root);
      if legacy.exists() && ... .map(|i| i.phase == phase).unwrap_or(true) {
          std::fs::remove_file(&legacy)?;
      }
  }
  ```
- **Defect:** Two related problems. (a) The existence pre-check looks only at the per-phase path, but `delete_cron_instructions` also deletes a legacy `cron-instructions.json` when it names this phase — so the emitted `path_kind: "per-phase"` event can describe a deletion that was actually the legacy record. (b) A phase whose only record is the legacy single-slot file (a pre-14a binary) has `cron_instructions_path(...).exists() == false`, so the resume-side deletion is skipped entirely and the legacy record survives a genuine relaunch. The plan's own text concedes the legacy path "is not knowable here," but then proceeds to emit a *definite* `"per-phase"` value rather than an honest `"unknown"`, and does not address the legacy-only-consumption hole.
- **Consequence & Reproduction:** Ship/resume a pre-14a-era rate-limited phase; the legacy record is neither consumed on resume (D-16) nor truthfully reported on deletion (D-18). Additionally `delete_cron_instructions`'s `.unwrap_or(true)` means a *corrupt* legacy file is deleted unconditionally, even if it was written for a different phase (pre-existing behavior, but directly relevant to D-18's "legacy if knowable" wording).

### M5 — `shell_quote` is unreachable from `devflow-cli`, and 44-03's quoting instruction risks double-quoting

- **Severity:** Medium (broken-instruction risk in the exact surface the phase is fixing)
- **Location:** `44-03-PLAN.md` Task 2 `<action>` (lines 268-272): *"Quote the embedded command with `ship::shell_quote` — or, if that helper is not reachable from this crate, with the same quoting discipline…"*. Versus `crates/devflow-core/src/ship.rs:374` (`fn shell_quote` — private, no `pub`) and `:184-187` (the `command` field is **already** `shell_quote`-protected).
- **Defect:** `shell_quote` is a private `fn` in `devflow-core`, so it is *not* reachable from `commands.rs` — the plan's first option is impossible, and its fallback ("same quoting discipline") is an unspecified re-implementation, i.e. exactly the "two copies, free to drift" class the plan's own Pattern 1 warns against. Worse, the instruction conflates two distinct quoting layers: the `command` field is already quoted at build time; re-running `shell_quote` on it would produce `'cd '\''/path'\'' && devflow resume --phase 7'` — a broken instruction.
- **Consequence & Reproduction:** A naive executor follows the letter of the instruction and double-quotes the already-quoted `command`, emitting a `hermes cron create` line whose prompt positional is corrupted — the precise defect #148 was filed to eliminate, reintroduced by the fix.

### M6 — 44-04's "non-fakable" evidence bar is mtime-only, and the classification task is un-gated and same-actor

- **Severity:** Medium (evidence-integrity weakness in the phase's only deliverable)
- **Location:** `44-04-PLAN.md` Task 2 acceptance criteria (lines 247-252): *"`44-evidence/` contains at least one file of raw agent output whose modification times fall inside the recorded run window."* Task 3 is `type="auto"` (line 268) with no `checkpoint:decision`. P-01/P-02 (lines 40-48) are prose prohibitions with `status: flagged-unverified`.
- **Defect:** The single mechanical discriminator between a real capture and a hand-authored one is **mtime**, which is trivially spoofable (`touch`). The attempt count is self-reported by the operator at the resume-signal, and the classification of the evidence (Task 3) is an *auto* task performed by the same executor that could have authored it. P-01/P-02 have no automated enforcement; the "modification times inside the run window" criterion is the only check, and it does not verify *content*.
- **Consequence & Reproduction:** An executor (or a careless operator) that writes a plausible transcript and `touch`es it inside the window passes every acceptance criterion. This is a weaker bar than the plan's own threat model (T-44-17, "evidence authored rather than captured") claims to close. The honest human checkpoint exists (Task 2 `blocking-human`), but it gates the *decision* to run, not the *verification* of what was captured.

---

## LOW

### L1 — `44-03` behavior leaves the positive-offset ISO expectation dangling
`44-03-PLAN.md:118` — *"`hermes_schedule_from_retry_after("2026-06-18T15:45:30+0530")` equals the `+05:30` result"* references a `+05:30` line that does not exist (line 117 is `-05:30`). The existing test it mirrors (`ship.rs:559-564`) is equivalence-only, so the rewritten ISO test risks inheriting equivalence-only coverage: a sign-symmetric offset bug affecting both `+0530` and `+05:30` identically would pass. The correct literal (`2026-06-18T10:16:00Z`) is never stated.

### L2 — Task 2 verify filter misses one of its five tests
`44-01-PLAN.md:314` filters on `resume_with_agent`, but the idempotent test is named `resume_with_same_agent_is_an_ordinary_idempotent_resume` (line 86) — it is "filtered out" of the very run the acceptance criteria (line 320) claim exercises "all five named tests." The `cargo test --workspace` line backstops it, but the naming/filter contract is internally inconsistent and the "non-zero filtered out" assertion the criteria demand is not actually encoded in the `<verify>` command.

### L3 — `44-02` frontmatter contradicts Task 3
`44-02-PLAN.md:10` lists `crates/devflow-core/src/recover.rs` in `files_modified`, while Task 3 (lines 266-271) mandates *"zero production-code lines changed (D-19)."* Cosmetic, but the metadata says "modified" where the plan says "must not be modified."

### L4 — 44-04 flattens two orthogonal decisions into one option list
`44-04-PLAN.md:218-239` presents four `<option>` entries that are actually two independent axes (target ∈ {phase-45, throwaway}, host ∈ {fresh-worktree, main-checkout}), yet the `<resume-signal>` (lines 241-245) correctly asks for *two* ids. A reader could parse it as a single four-way choice.

### L5 — The D-15/D-16 negative control lands a wave *after* the mutation it guards
The resume-side deletion (and its positive tracer test) is written in 44-01 (wave 1); the "record survives failed relaunch" negative control is deferred to 44-02 Task 1 (wave 2). A premature-deletion bug ships to the branch in wave 1 before its guard test exists. Not a correctness defect, but the wave structure inverts the "negative control before the mutation" discipline the plans themselves preach.

### L6 — `recover::clean`'s legacy-deletion `.unwrap_or(true)` is out-of-scope collateral for D-18
`ship.rs:149` — `.map(|i| i.phase == phase).unwrap_or(true)` deletes an *unparseable* legacy `cron-instructions.json` unconditionally, even when it was written for a different phase. Pre-existing and untouched by D-19, but it means the `delete_cron_instructions` primitive the phase is reusing is not strictly phase-scoped for the legacy path — relevant to D-18's "legacy if knowable" honesty claim.

### L7 — The "immediate" D-07 save is redundant; the "race" it guards doesn't exist
`44-01-PLAN.md:170-171` adds an immediate `workflow::save_state` in the handoff branch, but `resume()` already calls `workflow::save_state(&state)?` at `pipeline_launch.rs:1284` before `launch_stage`. Both are in the same synchronous process before any detached spawn, so there is no race between state-write, event-emit, and process-spawn to close. The double-write is harmless, but it signals the plan is defending against a concurrency hazard that is not present — which weakens confidence that the *actual* ordering hazards (post-persistence launch failure, M2) were identified.

---

## Direct answers to the seven hunt questions

1. **Live-code discrepancies** — No hallucinated APIs or wrong function names; line bounds are essentially all correct. The genuine omissions are *unlisted call sites* (M1) and *unspecified branches* (H1), not stale references.
2. **Handoff semantics** — Run identity is preserved (D-06 field list is verified complete against `State`). There is no real race (single-threaded, L7). The open failure mode is the half-handed-off state + false event on post-persistence launch failure (M2) and the omitted driver-availability pre-check (M3).
3. **Refusal/preflight** — The interactivity predicate is correctly reused (not duplicated), and the `Stage::Plan` non-refusal is correctly preserved. But "same seriousness as start" is overstated: `driver.health` is not pre-checked (M3), and the D-08 negative-control test only exercises the Auto-mode Define path, leaving the Supervise-mode Define handoff (which the existing predicate deliberately permits) untested.
4. **Cron contract** — The ISO-8601-`Z` fix is sound and verified against `parse_schedule` (the `dt.tzinfo is None` gate at jobs.py:800 behaves exactly as claimed). Unparseable retry stays fail-closed at the *value* level. The gap is the *rendering* of that empty value (H1).
5. **Cron lifecycle/cleanup** — Deletion is correctly tied to the post-`monitor_pid` write; the failed-relaunch case preserves the record (correctly designed). Ship-side handles both per-phase and legacy via the idempotent primitive, but the legacy path introduces the D-18 misstatement and the legacy-only-consumption hole (M4). `recover::clean` is correctly left untouched.
6. **Codex dogfood rigor** — There *is* a real `blocking-human` checkpoint (Task 2), and the plan honestly forces "completed vs surfaced-gap" framing (D-03) plus a "what this does not establish" section. But the mechanical anti-fakability check is mtime-only and the classification task is un-gated and same-actor (M6).
7. **Negative controls/verifiability** — The plans are unusually strong here: `resume_without_agent_leaves_the_saved_agent_untouched`, the over-refusal Pitfall-1 control (Task 2 Test 2), the paired positive control (44-02 Test 3), and the D-14 bare-cron-fields divergence test are all genuine. The remaining holes: no empty-schedule rendering control (H1), no driver-absent handoff control (M3), and the positive-offset literal gap (L1). The plan also correctly flags the vacuous `assert_ne!(schedule, "* * * * *")` becoming vacuous under the ISO format (44-03 lines 178-182) — that is a rare instance of a plan catching its own proxy-measurement trap.

The phase is well-researched and mostly accurate at the citation level; its real risk is concentrated in **H1** (an unspecified branch that re-creates the exact bug class being fixed) and the **M2/M3** handoff-audit-integrity pair. Those three should be resolved before execution, not deferred to "Claude's discretion."
