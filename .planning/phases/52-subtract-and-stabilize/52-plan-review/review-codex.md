## CONFIRMED

**F1 — High — An uncovered quick fix can finish without being re-verified.**

- **Location:** `.planning/phases/52-subtract-and-stabilize/52-29-PLAN.md:178`; `52-28-PLAN.md:24`.
- **Quoted text:** “a non-empty remainder routes to the … stale re-verify relaunch and post-check”; “if it is `passed` or `gaps_found`, Validate runs”.
- **Why it breaks:** Add an uncovered file, launch re-verification, let the agent skip the verifier, and return the unchanged report. GSD still reports `passed`; the post-check accepts it. The proposed negative control only covers an actually `stale` report.
- **Evidence checked:** `52-GAP-LOOP-VERIFICATION.md:92–100` records both ingredients: an agent skipped re-verification, and adding an uncovered file left status `passed`. The source’s report-location/fingerprint helpers are in `crates/devflow-core/src/agent_result.rs:3817` onward.
- **Suggested fix:** Persist the outstanding re-verification obligation and require fresh GSD evidence tied to the fix before clearing it. Add a control where the re-verification agent returns success while leaving the old `passed` report unchanged.

**F2 — High — Approve can finish against verification evidence that changed during the wait.**

- **Location:** `52-29-PLAN.md:34`.
- **Quoted text:** “`approve_route` … follows the recorded status … `passed` → finish”.
- **Why it breaks:** Open a clean-pass gate, change a covered file or replace the report while waiting, then approve. Both routers still consume the gate’s old `passed` value. The run finishes although a current GSD read would return `stale` or `gaps_found`. This defeats D-08’s verification guarantee.
- **Evidence checked:** `crates/devflow-cli/src/pipeline_gate.rs:633–692` waits, claims the answer, and maps it without refreshing verification. `config_parse.rs:16–43` allows a three-day default wait. No later plan requires a freshness check at approval pickup.
- **Suggested fix:** Refresh verification before consuming a finish approval. If the evidence changed, refuse that approval and reopen with the current basis. Test both unchanged and changed evidence during a pending gate.

**F3 — High — Plan 16 requires a green suite before Plan 17 repairs a test it necessarily breaks.**

- **Location:** `52-16-PLAN.md:82`, `52-04-PLAN.md:97`, `52-17-PLAN.md:160`.
- **Quoted text:** “Set `State::new`’s stage to `Stage::Plan`”; “If the witness keeps `--until define`, leave it”; the witness changes to `--until plan` only in Plan 17.
- **Why it breaks:** After Plan 16, no run reaches Define, so that witness’s stop condition cannot fire. Nevertheless, Plan 16’s final gate runs `scripts/check.sh all`.
- **Evidence checked:** `crates/devflow-cli/tests/supervisor_e2e.rs:439–451` passes `--until define` and waits for stopped state. `pipeline_gate.rs:73` halts only when `stop_until == Some(from)`.
- **Suggested fix:** Move the affected e2e migrations into Plan 16 alongside the start-stage change. Plan 17 should only remove the now-unused variant.

**F4 — High — The stage-removal tasks cannot pass their own gates in the specified order.**

- **Location:** `52-15-PLAN.md:88–89`, `:115`, `:176`; analogous split at `52-17-PLAN.md:83`.
- **Quoted text:** “until Task 2 lands the CLI crate does not build”; “commit Tasks 1 and 2 together **if** … [a] hook builds the workspace”.
- **Why it breaks:** Workspace compilation is a correctness requirement regardless of hook configuration. Additionally, Plan 15 Task 1 removes review-angle configuration and runs the entire core library suite, while the documentation removal is deferred to Task 3. Its `doc_check` gate therefore encounters documentation naming removed source identifiers.
- **Evidence checked:** CLI `Stage::Ship` consumers remain in `pipeline_launch.rs:4003` and elsewhere. `doc_check.rs:365–372` rejects documented environment variables absent from source. Review-angle mentions exist in `README.md:175`, `ARCHITECTURE.md:441`, and `docs/guides/configuration.md:50`.
- **Suggested fix:** Make variant removal, all consumers, affected tests, and corresponding documentation one unconditional atomic task/commit. Run workspace verification at that boundary.

**F5 — High — Historical events make a fresh run look prepared.**

- **Location:** `52-16-PLAN.md:112–114`.
- **Quoted text:** “true when … no `workflow_started` event exists **for the phase**”.
- **Why it breaks:** Run a phase once, then start it again and interrupt preparation. The earlier run’s event remains, so the fresh reservation is classified as already launched. The proposed tests only exercise a fresh history.
- **Evidence checked:** `crates/devflow-core/src/events.rs:1–17,63–67` defines an append-only project log. `commands.rs:1148–1169` emits `workflow_started` without a run-generation identity. The existing `run_never_prepared` at `commands.rs:888–890` instead uses persisted preparation evidence.
- **Suggested fix:** Use preparation state belonging to the current run generation. Do not infer it from any historical event for that phase. Add a previous-run-history control and an unavailable-event-log case.

**F6 — High — Stopping a gap pass destroys the information needed to resume it correctly.**

- **Location:** `52-27-PLAN.md:101–103`.
- **Quoted text:** “a persisted gap-closure marker … cleared when … the run stops”.
- **Why it breaks:** Stop while the gaps Plan or Code stage is pending, then resume. Clearing the marker selects the ordinary Plan/Code intent, losing `--gaps` or `--gaps-only`. Ordinary Plan can short-circuit because plans already exist—the exact trap this plan otherwise addresses.
- **Evidence checked:** `pipeline_launch.rs:3650–3669` clears the stopped condition and relaunches the saved stage. It cannot reconstruct a deleted gap intent. `pipeline_gate.rs:73–86` also supports intentional intermediate halts.
- **Suggested fix:** Preserve gap intent across resumable stops and interruptions. Clear it on successful completion, explicit abandonment, or replacement by another fix route. Add stop/resume controls at both gaps stages.

**F7 — High — Plans 29 and 30 prescribe mutually incompatible context tests.**

- **Location:** `52-29-PLAN.md:280`; `52-30-PLAN.md:86–87`; reinforced by `52-32-PLAN.md:95`.
- **Quoted text:** Plan 29 requires the context to “**end** with exactly” its answer sentence and forbids answer words before it. Plan 30 requires the answer sentence “**first and the cause last**”.
- **Why it breaks:** A nonempty failure context cannot satisfy both. Moving the sentence first makes Plan 29’s test fail; leaving it last loses the operator’s instructions under the 300-character display/notification cap.
- **Evidence checked:** `pipeline_outcomes.rs:920–945` contains the differing context builders; `commands.rs:2056` defines the display cap. Plan 30 runs the full suite without replacing Plan 29’s incompatible assertion.
- **Suggested fix:** Keep the shared router renderer, require its sentence first everywhere, and update all parity tests and Plan 32’s rate-limit row accordingly.

**F8 — Medium — The refused-answer recovery path must reopen a gate the open API forbids.**

- **Location:** `52-29-PLAN.md:245–246,277`; `52-22-PLAN.md:225–227`.
- **Quoted text:** Reopen “with the kind and status it was opened with”, including “a file with no kind”; the earlier plan makes the opening parameter “a `GateKind`, **not an `Option`**”.
- **Why it breaks:** The required kindless bypass test cannot reopen the same kind through the required API. No `Unclassified` gate kind is specified. Implementers must either weaken the opening invariant or invent an unplanned recovery representation.
- **Evidence checked:** `crates/devflow-core/src/gates.rs:468–483,1301` contains the opening/writing path that Plan 22 changes. Plan 29 explicitly includes missing-kind records in its supervisor bypass tests.
- **Suggested fix:** Specify a typed recovery kind with abort-only routing, or retain the original gate while rejecting its invalid answer. Test that chosen behavior through the required opening API.

**F9 — High — The timer design implements at-most-once entitlement consumption, not exactly one automatic relaunch.**

- **Location:** `52-32-PLAN.md:27,104–106`.
- **Quoted text:** “commits `rate_limit_auto_relaunch_used = true` first”; “A crash between the two writes can lose the automatic relaunch”.
- **Why it breaks:** Crash after persisting the flag but before publishing the timer answer. Recovery sees the entitlement as spent although no timer answer won and no automatic launch happened. This contradicts D-38’s meaning of the flag and CUT-03’s exactly-once claim. The listed takeover tests start from an already-used flag; they do not exercise this boundary.
- **Evidence checked:** `gates.rs:1188–1203` durably publishes the consumption claim separately from state. `pipeline_gate.rs:633–692` likewise separates answer pickup, claim, and state mutation.
- **Suggested fix:** Specify a recoverable pending timer action keyed by gate identity, with idempotent publication and recovery. Inject crashes between each durable step; do not label the current two-write protocol exactly-once.

## SUSPECTED

**S1 — High — Speculation: relocating the reset parser preserves an invalid-offset-to-UTC fallback.**

- **Location:** `52-01-PLAN.md:121–123`.
- **Quoted text:** “Move `parse_rfc3339ish`, `split_time_and_offset` and `parse_offset_minutes`”.
- **Why it may break:** The source helper converts an invalid explicit offset into zero. For example, `2026-08-02T00:00:00+99:00` can become a valid UTC deadline instead of the plain gate required by D-27.1. The planned invalid-input tests do not exercise malformed RFC 3339 through the complete parser.
- **Evidence checked:** `crates/devflow-core/src/ship.rs:418,426` uses `parse_offset_minutes(...).unwrap_or(0)`. Its calendar check at `:388` also accepts impossible dates such as February 31.
- **Suggested fix:** Propagate invalid offsets as failure, validate calendar dates, and add full-parser negative controls paired with valid offset/leap-day controls. This remains suspected because the new parser has not been implemented.

## COVERAGE

- **Read in full:** Plans 52-08–10, 52-15, 52-17–18, 52-20, 52-27–32.
- **Skimmed with targeted task, gate, dependency, and source checks:** Plans 52-01–07, 52-11–14, 52-16, 52-19, 52-21–26.
- Checked the supplied CONTEXT decisions, requirements, Phase 52 roadmap criteria, inventory, validation contract, research, patterns, and `CLAUDE.md` against relevant source. These findings are not an exhaustive certification of coverage or scope.
- No Rust suite, container check, or semver check was run. “Confirmed” denotes contradictions or failure paths established from the specified plans and inspected source, not executed future implementations.
- No repository files were edited or temporary files created; probe children were reaped. The snapshot lacks Git metadata, so commit `e6b3343` and branch identity could not be independently verified.
