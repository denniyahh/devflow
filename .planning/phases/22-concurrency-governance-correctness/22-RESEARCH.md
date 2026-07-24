# Phase 22 Research: Concurrency/Governance Correctness

## User constraints

- **[High | user/22-CONTEXT.md]** This is a narrow dogfooding trial for backlog **999.30 / DEN-55**, not a full concurrency-design phase.
- **[High | user/22-CONTEXT.md]** Scope is exactly: share omitted-stage gate resolution between `gate_show` and `gate_respond`; use `devflow_core::config::MAIN` instead of literal `"main"`; make `gate_show` perform one open-gate read; and extend the single-pass status event summary with the latest `stage_launched` timestamp.
- **[High | user/22-CONTEXT.md]** Preserve CLI output and gate semantics, reuse helpers/types, add no dependencies, add focused regression coverage, and validate with fmt, clippy, relevant tests, and the workspace suite before stopping at Validate.
- **[High | user/22-CONTEXT.md]** Explicitly defer `--base` (999.28), concurrent version-tag contention (999.4), object-store races (999.26), and unrelated backlog work.
- **[High | task]** This artifact is research only. Do not edit source; produce only this file.

## Evidence and implementation seams

### 1. Shared omitted-stage gate resolution

`commands.rs:741-805` (`gate_respond`) and `commands.rs:814-855` (`gate_show`) independently filter `Gates::list_open(project_root)` by phase and implement identical `[]` / `[one]` / `many` behavior and messages. The `gate_show` doc comment at `commands.rs:810-813` claims the behavior cannot drift, but the code is duplicated. **[High | 21-REVIEW.md, source inspection]**

Recommended seam: add a private `resolve_single_open_gate_stage(project_root: &Path, phase: u32) -> Result<Stage, CliError>` near the gate handlers. It should own the existing error strings and return the single stage. Both callers should use it only when `stage` is `None`; explicit `Some(stage)` behavior remains unchanged. **[High | 21-REVIEW.md proposed fix]**

Important distinction: this helper alone cannot satisfy the one-read `gate_show` requirement if it calls `Gates::list_open` internally and `gate_show` then reads again. Either (a) let `gate_show` fetch once and resolve from its fetched `Vec<OpenGate>`, while `gate_respond` uses a shared slice-based resolver, or (b) define a resolver over `&[OpenGate]` plus a small fetch/filter wrapper. The latter makes reuse and one-read behavior explicit. **[High | source control-flow analysis]**

### 2. `gate_show` one-read behavior

Current `gate_show` reads open gates once for omitted-stage inference (`commands.rs:819-844`) and again to find the selected gate (`commands.rs:846-852`). Between reads, another process can answer/remove the gate, producing a false “no open gate” error. **[High | 21-REVIEW.md WR-03]**

Recommended shape: `let open = Gates::list_open(project_root);`, derive the phase-filtered view from `open`, resolve omitted stage from that same data, then find the matching `OpenGate` in `open`. Preserve the explicit-stage lookup and existing not-found message. Do not broaden this into locking or a transactional gate protocol; those are outside the trial. **[High | 21-REVIEW.md, 22-CONTEXT.md]**

### 3. `MAIN` constant

`commands.rs:2276-2288` (`collect_planning_doc_findings`) currently calls `tag_exists_and_reachable(project_root, tag, "main")`. `devflow-core/src/config.rs:15` defines `pub const MAIN: &str = "main"`; `commands.rs` already imports `DEVELOP` and `FEATURE_PREFIX` at line 22, so extend that import to include `MAIN`. The surrounding docs at `commands.rs:2268-2275` should refer to `MAIN`/the named production branch without changing the read-only reconciliation semantics. **[High | 21-REVIEW.md WR-02, source inspection]**

This is source-of-truth hygiene, not a current output change: today `MAIN` equals `"main"`. Do not replace it with `GitFlowConfig.main` or introduce config loading; the requested fix is the existing constant. **[High | 21-REVIEW.md, 22-CONTEXT.md]**

### 4. Single-pass status event summary

`crates/devflow-core/src/events.rs:73-92` implements `last_events_by_phase`, reading/parsing `events.jsonl` once and retaining each phase’s latest event. `commands.rs:589-591` already relies on this optimization for status’s “last action.” However, `commands.rs:547-564` (`latest_stage_launched_ts`) rescans the entire file, and `status()` calls it inside the per-phase loop at `commands.rs:648-654`; this is the exact regression identified as IN-01 in `21-REVIEW.md`. **[High | 21-REVIEW.md IN-01, source inspection]**

Recommended design: extend the core event pass to return a per-phase summary containing both the latest event and the latest valid `ts` from any `stage_launched` event. A named summary struct/map (rather than making callers rescan JSON) is the clearest contract; retain or adapt `last_events_by_phase` for existing consumers if minimizing API churn is important. In `status()`, remove `latest_stage_launched_ts` and consume the summary’s timestamp for `render_stage_progress_line`. `collect_phase_facts` also consumes the latest-event map, so account for its current call site when choosing the compatible return type. **[Medium | source inspection; implementation choice]**

The timestamp must be the most recent matching `stage_launched` event, not necessarily the latest event overall: a later `transition`, `gate_fired`, or corrupt line must not erase a valid launch timestamp. Parse failures remain skipped, matching `last_events_by_phase`. Missing log/file or no matching launch yields `None`, preserving the current rendering (`in stage <stage>` without an age). **[High | commands.rs:553-564 tests and events.rs behavior]**

## Focused regression coverage

- **[High | existing tests, `commands.rs:2443-2465`]** Preserve/extend `gate_show_errors_naming_gate_list_when_no_open_gate`, `gate_show_errors_asking_for_stage_with_several_open_gates`, and `gate_show_auto_resolves_single_open_gate`.
- **[High | existing tests, `commands.rs:2770-2804`]** Preserve `gate_respond_auto_resolves_single_open_gate` and its none/ambiguous/explicit-stage coverage; use these to prove the shared resolver retains exact semantics.
- **[High | new focused test]** Exercise `gate_show`’s selected gate from the same fetched open-gate collection indirectly through a phase with one gate and assert success; avoid brittle filesystem-call-count tests unless a test seam is already available.
- **[High | source/test gap]** Add a `MAIN`-specific regression at the pure reconciliation seam if practical: prove the lookup receives the named production branch without changing public CLI output. If testing the closure is awkward, a compile-level import plus existing planning-doc tests is sufficient for this low-risk constant substitution.
- **[High | existing tests, `commands.rs:2995-3064`]** Replace/update the `latest_stage_launched_ts_*` tests around the new summary API: no event → `None`; launch timestamp differs from `State::started_at`; latest of multiple `stage_launched` events wins; later non-launch event does not clear the timestamp; corrupt lines are ignored.
- **[High | existing tests, `events.rs:198-222`]** Extend the core `last_events_by_phase_collects_latest_per_phase_in_one_pass` coverage to assert the new stage-launch summary while retaining latest-event behavior.

## Pitfalls and validation notes

- **[High]** Do not alter gate wording, stage ordering, explicit `--stage` semantics, response-file behavior, or output formatting; Phase 21 review called these quality fixes, not behavior changes.
- **[High]** A shared helper that performs its own read can accidentally preserve `gate_show`’s two-read TOCTOU window. Keep the fetched `Vec<OpenGate>` alive through resolution and selection.
- **[High]** Do not use the last event’s timestamp as the stage age. The latest event may be unrelated to launch; track the latest matching `stage_launched` timestamp during the same pass.
- **[Medium]** Avoid changing `last_event_for_phase` semantics or introducing a second full-file reader. Preserve invalid-line fail-soft behavior and the existing one-pass rationale (14-CR-10).
- **[High | 22-CONTEXT.md]** No changes to `--base`, version-tag races, object-store concurrency, dependencies, release operations, or remote state.
- **[High | 22-CONTEXT.md]** Expected verification after implementation: `cargo fmt --check`, clippy with warnings denied, focused CLI/core tests, then workspace tests. Research does not run or claim those validations.

## Confidence and provenance

- **[High confidence]** Four requested edits and their current source locations: directly verified in `22-CONTEXT.md`, `21-REVIEW.md`, `commands.rs`, `events.rs`, and `ROADMAP.md`.
- **[High confidence]** Existing gate and timestamp regression tests: directly verified by `rg` and source inspection in `commands.rs`.
- **[Medium confidence]** Event-summary return-type recommendation: reasoned design guidance based on the two current consumers; exact API shape should be chosen during planning to minimize churn.
- **[High confidence]** Out-of-scope boundaries and Validate-only stopping point: directly stated in `22-CONTEXT.md`.

## RESEARCH COMPLETE
