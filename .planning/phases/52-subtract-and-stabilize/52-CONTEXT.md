# Phase 52: Subtract and Stabilize - Context

**Gathered:** 2026-11-08
**Status:** Ready for planning

<domain>
## Phase Boundary

After this phase a DevFlow run is Plan → Code → Validate and ends there as *finished*. There is no Define, no Ship, no
after-ship hooks, no cron-resume file, no `devflow ship`, no `parallel`, and none of the cut rows confirmed below
(D-01). The tree passes `scripts/check.sh all`. The supervise gate loop works end to end:
- the operator's rejection note reaches the relaunched agent verbatim;
- a run aborts only on an explicit abort answer;
- a `gaps_found` verification is driven to `passed` inside the run.

A rate limit with no fallback waits at a gate and relaunches by itself after the reset time. `--version`, `cleanup`,
`status`, `doctor` and the command output tell the operator what the run did and why it stopped.

Requirements: CUT-01..04, GATE-01..04, OPS-01..04 (`.planning/REQUIREMENTS.md`). The subtraction is the first plan wave,
ahead of any GATE or OPS fix (ROADMAP sequencing note). New capabilities belong in Phases 53-55.

</domain>

<decisions>
## Implementation Decisions

### Cut scope (CUT-04)
- **D-01:** The operator confirmed **all 19** of the inventory's remaining proposed `cut` rows (2026-11-08). The ROADMAP
  says "21 proposed"; that count predates the operator's `parallel` decision, which settled two of them (`parallel` and
  `parallel.rs`). With the 9 rows already decided by the operator (Define, Ship, the four after-ship hooks, the cron
  file, `devflow ship`, `--yes-ship`, `DEVFLOW_YES_SHIP`, `parallel`, `parallel.rs`), all **28** rows marked `cut` in
  `.planning/user/V4-CUT-INVENTORY.md` §1-4 are removed (counted with `rg '\| cut \|'`: 28 total, 9 marked
  "operator 2026-11-08"). The 19 confirmed rows:
  - **Ship-dependents:** `devflow evidence`, `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`,
    `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, `DEVFLOW_REVIEW_ANGLES`, the `DocsUpdate` hook, the finalization retry gate,
    the `gh auth` and major-bump preflight checks, `lock.rs`, and `version.rs` / `ship.rs` / `ship_evidence.rs`.
  - **Legacy paths:** `--legacy-claude-launch` on `start` and `resume`, `DEVFLOW_CLAUDE_LEGACY_LAUNCH`, the legacy Claude
    launch code path, the hidden no-op `start --worktree`, the positional `STAGE_OR_PROJECT`/`PROJECT` args on
    `gate approve`/`reject`, and `gate sweep --reap-strays`.
  - **Standalone commands:** `reference`, `list`, `test`, `release --check` / `--verify`.

  Rows marked `core`, `core-rework` or `defer` are out of this cut. `defer` rows stay as they are.
- **D-02:** `release --check`/`--verify` is cut **together with** moving its pin checks into `scripts/cut-release.sh`
  (it calls `run_devflow release --check` at L88 and `--verify` at L335), or into a selftested Rust tool, in the same
  change. The release script must not break. This is repo tooling under the operator's "avoid shell-check gates" rule,
  so the planner chooses between script and Rust tool and says why.
- **D-03:** Removed surface is deleted, not allowlisted around: its tests, OPERATIONS.md rows,
  README/ARCHITECTURE/CONTRIBUTING mentions and `doc-check-allowlist.toml` entries all go (ROADMAP criterion 2). These
  removals are already operator-decided and the 4.0.0 major bump is the accepted semver cost (ROADMAP v4.0 header). Do
  **not** turn them into `checkpoint:decision` tasks: DevFlow's one-shot launch cannot answer one (999.57), and auto
  preflight refuses `blocking-human` plans.

### How a run finishes (CUT-01, CUT-02)
- **D-04:** Supervise mode stays, kept minimal: **one gate, at every Validate**. Approve records `workflow_finished` with
  `shipped: false` and leaves the worktree and branch in place. Reject (with a note) runs the in-run fix loop (D-09 to
  D-11). The gate text no longer says "approve to ship?"; it asks to approve finishing. Supervise gets **no** Plan→Code
  gate: `--until plan` stays the way to stop after Plan.
- **D-05:** Auto mode finishes on a passing Validate (D-08) without a gate. The existing failure caps still force a
  gate (D-12).
- **D-06:** On `workflow_finished`, in either mode, DevFlow runs `DEVFLOW_GATE_NOTIFY_CMD` **once** with a "finished"
  payload and records `notify_fired`. This replaces the end-of-run signal the Ship gate used to give an unattended
  operator.
- **D-07:** There is no `--from` flag. A run always starts at Plan, which already accepts pre-written plans (8-9 s in the
  2026-11-08 probe), and `resume` covers restarts mid-run. `--until` accepts `plan`, `code` and `validate`; clap
  rejects `define` and `ship` as unknown values.

### Validate verdict and gap loop (GATE-02, GATE-03)
- **D-08:** The Validate verdict is **pass only if** the validate-phase agent says pass **and** GSD's own reader,
  `gsd query verification status <phase_dir>`, returns `passed`. DevFlow does **not** parse VERIFICATION.md frontmatter
  itself: GSD's reader applies the staleness rules (#2348, #4682, #4817), which a frontmatter parse would miss. DevFlow
  follows the route GSD returns:
  - `gaps_found` → the gap loop (D-09).
  - `stale` or `missing` → Code re-runs `/gsd-execute-phase N`, which resumes at the verification gates and re-runs the
    verifier (gsd-core 1.20.1 `bin/lib/verification.cjs` L257-290, `STALE_REMEDY`), then Validate.
  - `human_needed` → a gate in both modes, with the status in the gate context.

  The Validate gate never reads "passed" while GSD reports anything else. **Not established:** that execute-phase
  re-verifies a stale `passed` report when every plan already has a SUMMARY. This rests on the gsd-core comments and
  route table, not on a run. A test or the live run must show it.
- **D-09:** The gap loop runs at stage level: a `gaps_found` Validate goes to **Plan** running
  `/gsd-plan-phase N --gaps`, then **Code** running `/gsd-execute-phase N --gaps-only` (whose verifier tail
  re-verifies), then Validate. Each step is its own launch with its own `DEVFLOW_RESULT` and commit checks. The
  loop-back target for gaps therefore changes from Code to Plan. Closes 999.205.
- **D-10:** Rejecting a Validate that passed cleanly (no gaps) relaunches **Code** with the operator's note verbatim as
  the task, run through `/gsd-quick`, then Validate. `devflow gate reject` prints which route the rejection will run
  (999.189 fix shape). After such a fix the report is `stale`, so D-08's `stale` route covers re-verification.
- **D-11:** A `gaps_found` report whose covered files have changed since it was written (999.208) produces a **gate**
  whose context says the report is gaps_found but stale and that no GSD route re-verifies it. A gsd-core upstream
  issue is filed for `gaps_found` taking priority over `stale` (`verification.cjs` L1537). DevFlow does not drive a
  verifier outside a GSD command (operator principle 2026-09-28, "operate within GSD's confines"). The in-run gap loop
  does not hit this case, because its own execute-phase run re-verifies.
- **D-12:** No new cap. Each `gaps_found` round counts as a Validate failure, so `MAX_CONSECUTIVE_FAILURES` (3) and
  `MAX_PHASE_VALIDATE_FAILURES` (10) in `crates/devflow-core/src/mode.rs` apply unchanged.

### Gate answers (GATE-01, GATE-04)
- **D-13:** A new verb, `devflow gate abort <phase> [--gate-id <id>] [--note <text>]`, writes a gate response with an
  **explicit abort field**. `gate reject` always means fix and relaunch. `GateAction::from_response` (`gates.rs` L97)
  no longer matches the substring `abort`, so a note such as "do not abort, fix the test" keeps the run alive.
  `gate sweep` (`Gates::reap_answer`, `gates.rs` L680, which today writes the note `"abort: …"`) sets the same
  explicit field. The new verb is a `Command` variant, so it needs docs or an allowlist entry for `doc_check`.
- **D-14:** The rejection note reaches the relaunched agent's prompt **verbatim**, and that is asserted on the launch
  record or the captured prompt (ROADMAP criterion 4). Today `fix_prompt(fix_type, phase)` (`prompt.rs` L713) takes no
  note, so it gains one.

### Rate limit with no usable fallback (CUT-03)
- **D-15:** A rate limit with no usable fallback stops at a **real gate**:
  - it shows in `gate list`/`gate show` and the notify hook fires;
  - its context names the reset time and the exact `devflow resume --phase N …` command;
  - `gate approve` relaunches the same stage on the same agent now, and `gate abort` ends the run.

  The supervisor **also relaunches by itself once `resetsAt` has passed** (plus a small margin). This replaces the
  cron-instructions file, which nothing ever acted on (999.186). No `cron-instructions-NN.json` is written.
  — **Reversibility:** costly — Phase 55's rotation will reuse this wait-and-relaunch gate for an exhausted rotation,
  so changing its shape later touches both phases.
- **D-16:** Limits on that relaunch:
  - with no `resetsAt` on the denial, it is an ordinary blocking gate and nothing is guessed;
  - with a known `resetsAt`, there is **exactly one** automatic relaunch;
  - if that relaunch is denied again, the gate stays open for the operator.

  No loop and no new config key. The wait is bounded by `DEVFLOW_GATE_TIMEOUT_SECS` like any gate.
- **D-17:** The requirement text must be amended to match D-15. CUT-03 in `.planning/REQUIREMENTS.md`, Phase 52
  criterion 3 and Phase 55 criterion 1 in `.planning/ROADMAP.md` currently say the run "stops at"/"ends at" a gate.
  They become "waits at a gate that relaunches at the reset time (once; no reset time → plain gate)". Phase 52's
  criterion 3 control keeps its "one fixture, both directions" shape and also asserts the single automatic relaunch.

### How this phase is run (operator, 2026-11-08)
- **D-18:** Phase 52 is **not dogfooded**. It runs with GSD commands in an agent session (`/gsd-plan-phase 52`,
  `/gsd-execute-phase 52`, …), not with `devflow start`. This is an exception to the CLAUDE.md dogfooding rule, which
  says DevFlow drives its own phases from Phase 51 on, and is recorded here as that rule requires. The criterion 1
  control (a `--dry-run` of the installed 3.0.0 binary) is a dry run, not a dogfood run, and still applies.
- **D-19:** **Verify the gap loop before building it.** No plan task may implement D-08..D-11 (the verdict from GSD's
  status, the stage-level gap loop, the clean-reject route, the stale-gaps gate) until a recorded check on a throwaway
  GSD project has shown what GSD actually does:
  - `query verification status` returns `passed` / `stale` / `gaps_found` in the cases D-08 assumes;
  - `/gsd-plan-phase N --gaps` writes `gap_closure` plans from a `gaps_found` report;
  - `/gsd-execute-phase N --gaps-only` executes them and its verifier re-verifies to `passed`;
  - `/gsd-execute-phase N` on a stale `passed` report with every plan summarized re-runs the verifier.

  The record lives at `52-GAP-LOOP-VERIFICATION.md`. If it contradicts a decision, that decision returns to the
  operator before planning continues.

### Claude's Discretion
- How the note is framed inside the prompt: a delimited verbatim block, with control characters other than newline and
  tab stripped. It must stay byte-identical for printable text.
- The margin after `resetsAt`, how the wait is implemented inside the supervisor, and the exact resume-command shape
  (it includes `--agent` when the run was not on the default agent).
- Whether `stamp_validate_dispatch_window` / `select_loop_back_fix` (`pipeline_launch.rs` L112,
  `pipeline_outcomes.rs` L318) survive. Routing now keys off GSD's status (D-08), so the authorship heuristic behind
  999.192 may be removable. If it is kept, a verifier-written report from Code must route correctly (999.192's test).
- The OPS-01..04 details (wording, layout), within the requirement text and the ROADMAP criterion 5 controls.
- Wave split after the cut wave.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Scope and requirements
- `.planning/ROADMAP.md` § "v4.0 milestone" and § "Phase 52: Subtract and Stabilize": goal, sequencing, the 5 success
  criteria and their controls.
- `.planning/REQUIREMENTS.md` § Subtraction (CUT), § Supervise gate loop (GATE), § Operator surface (OPS).
- `.planning/user/V4-CUT-INVENTORY.md`: the cut list. Every `cut` row in §1-4 is confirmed (D-01). §7 says what the
  inventory does not establish.

### Backlog entries closed or retired here (`.planning/ROADMAP.md` `### Phase 999.N`)
- 999.189 (rejection note dropped; "abort" substring), 999.192 (report read as inherited → FullExecute), 999.193
  (gate says passed over `gaps_found`), 999.205 (no loop-back plans gap closure), 999.208 (stale `gaps_found` can't be
  re-verified), 999.206 (300-char gate reason).
- 999.183, 999.214 (Define), 999.180, 999.210 (Ship), 999.185, 999.186 (cron file), 999.181 (parallel).
- 999.213 (cleanup), 999.190, 999.202 (status), 999.216 (`--version`), 999.217 (doctor/tracing noise).

### GSD behaviour DevFlow now depends on
- `~/.claude/gsd-core/bin/lib/verification.cjs` (gsd-core 1.20.1): `VERIFICATION_ROUTES` and `STALE_REMEDY` L257-290;
  `gaps_found` over `stale` priority L1537.
- `~/.claude/gsd-core/workflows/execute-phase.md` L370-397: execute-phase reads `verification status` and resumes at the
  gates for a stale report.

### Prior phase decisions carried forward
- `.planning/milestones/v3.0.0-phases/51-rate-limit-agent-failover/51-CONTEXT.md`: D-04..D-10 (failover handoff, one
  hop, the worktree handed over as-is), which CUT-03's gate sits beside.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `Mode::should_gate` (`crates/devflow-core/src/mode.rs` L243): the `Stage::Ship => true` arm goes. Supervise's
  Validate arm and the two failure caps stay as they are.
- `GateAction` / `GateAction::from_response` (`crates/devflow-core/src/gates.rs` L85-110): gains an explicit abort
  input (D-13).
- `fix_prompt` and `FixType` (`crates/devflow-core/src/prompt.rs` L713): gain the note (D-14) and a quick-fix arm (D-10).
  Plan gains a `--gaps` prompt (D-09).
- `resetsAt` is already parsed from the `rate_limit_event` (`crates/devflow-core/src/agent_result.rs` ~L1778-1786) for
  D-15.
- `GATE_CONTEXT_DISPLAY_CHARS = 300` (`crates/devflow-cli/src/commands.rs` L2056) and `gate_show` (L4434): the 999.206
  fix.
- `Stage` enum (`crates/devflow-core/src/stage.rs` L16): `Define` and `Ship` are removed (a semver-major `pub` change; run
  `cargo semver-checks` per CLAUDE.md).

### Established Patterns
- Source-scan Rust tests with comments stripped (`doc_check.rs` pattern) for "no literal X left" (ROADMAP criterion 2),
  with a planted stray literal as the control.
- Gate a plan on one named test: `cargo nextest run -p <pkg> --lib --no-tests=fail -E 'test(=full::path)'`. The CLI
  package is `devflow`, not `devflow-cli`.
- Every `std::env::var("DEVFLOW_…")` literal and every `Command` variant must be documented or allowlisted in the same
  change, or `doc_check` fails.

### Integration Points
- `crates/devflow-cli/src/pipeline_outcomes.rs` (cron record writer near L81; `select_loop_back_fix` L318) and
  `pipeline_launch.rs`: where the Validate outcome, loop-back routing and the rate-limit gate change.
- `crates/devflow-core/src/recover.rs` L266, L802: cron-instructions cleanup paths that go with the cron cut.
- `scripts/cut-release.sh` L88, L335: the `release` cut must keep this script working (D-02).

</code_context>

<specifics>
## Specific Ideas

- The operator considered dropping supervise entirely for an auto-only mode where Jev/Laya surfaces issues only when
  needed. It is kept minimal for now because triage does not exist until Phase 54 (see Deferred).
- Live-run control (ROADMAP criterion 1): record once in the phase notes that the installed 3.0.0 binary's dry run
  still lists Define and Ship.

</specifics>

<deferred>
## Deferred Ideas

- **Phase 54:** fold `auto`/`supervise` into a triage policy, where "supervise" = the operator answers everything,
  including the finish review, and "auto" = triage decides. Consider this in Phase 54's discuss step, once the model
  exists (operator, 2026-11-08).
- **Upstream (gsd-core):** `readVerificationStatus` returns `gaps_found` before the staleness check
  (`verification.cjs` L1537), so a gaps report whose covered files changed cannot be routed to re-verification.
  File it as a gsd-core issue (D-11).

</deferred>

---

*Phase: 52-subtract-and-stabilize*
*Context gathered: 2026-11-08*
