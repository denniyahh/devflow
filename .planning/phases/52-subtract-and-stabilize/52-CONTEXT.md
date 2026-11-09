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

  The Validate gate never reads "passed" while GSD reports anything else. The stale route was shown live by check 4 in
  `52-GAP-LOOP-VERIFICATION.md`: `execute-phase` re-ran the verifier with every plan summarized. Still untested live:
  the Validate stage's pairing of the agent's verdict with this status. Amended by D-24 (reader failures, evidence
  root).
- **D-09:** The gap loop runs at stage level: a `gaps_found` Validate goes to **Plan** running
  `/gsd-plan-phase N --gaps`, then **Code** running `/gsd-execute-phase N --gaps-only` (whose verifier tail
  re-verifies), then Validate. Each step is its own launch with its own `DEVFLOW_RESULT` and commit checks. The
  loop-back target for gaps therefore changes from Code to Plan. Closes 999.205.
- **D-10:** Rejecting a Validate that passed cleanly (no gaps) relaunches **Code** with the operator's note verbatim as
  the task, run through `/gsd-quick`, then Validate. `devflow gate reject` prints which route the rejection will run
  (999.189 fix shape). After such a fix the report is `stale`, so D-08's `stale` route covers re-verification.
- **D-11:** A `gaps_found` report whose covered files have changed since it was written (999.208) produces a **gate**
  whose context says the report is gaps_found but stale and that no GSD route re-verifies it. A gsd-core upstream
  issue is filed for `gaps_found` taking priority over `stale` (`verification.cjs` L1537): open-gsd/gsd-core#6078.
  How the case is detected is set by D-22. DevFlow does not drive a
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
  operator before planning continues. Checks 1-4 held. Check 5 contradicted D-08's stale route and D-10, and the operator resolved both with D-32 and D-33,
  so D-08..D-11 may now be planned. (Check 5, D-21, was: `/gsd-quick` on
  a passed phase, once on a covered file and once on an uncovered one, then `verification status` and execute-phase
  re-verify.)

### Amendments from the external review (2026-11-08)

Source: adversarial review of this file by codex (GPT-6.1 Sol), agy (Gemini 3.8 Flash) and DeepSeek Flash, in
`52-context-review/`. Every finding below was checked against the source before it was recorded. D-20..D-22 are
operator decisions; D-23..D-31 are within Claude's remit and correct facts or fill gaps without changing intent.

- **D-20 (operator): GATE-01 governs gate answers only.** A gate answer aborts a run only when it is an explicit abort
  (D-13). DevFlow's own infra-ceiling abort (`gate_or_abort_infra`, `pipeline_outcomes.rs` L60-78) stays: it bounds
  stuck loops and is never silent. GATE-01's text is amended to say this. Under D-15, a rate limit no longer counts
  toward the infra ceiling: it goes to the rate-limit gate. Today it does count (`handle_rate_limited_outcome`, L91-102).
- **D-21 (operator): GATE-02 is amended for D-10.** A `gaps_found` verdict runs GSD's gaps route. Rejecting a clean
  pass runs `/gsd-quick` with the note. D-10 is also gated by a live check 5 under D-19: a quick fix makes GSD report
  `stale`, and execute-phase then re-verifies. The check must include a quick fix that touches a file the report does
  **not** cover, because staleness is only shown for covered files. If that case does not go stale, D-10 returns to
  the operator.
- **D-22 (operator): how the stale `gaps_found` case of D-11 is detected.** GSD's status reader cannot tell it apart
  (#6078), so DevFlow uses GSD's own readers. It reads `covered_files` and `covered_digest` with
  `gsd query frontmatter get <report> --field …`, recomputes the digest with
  `gsd query verification fingerprint <phase_dir> <files…>`, and fires the D-11 gate on a mismatch. DevFlow never
  parses the report itself. Both readers were confirmed on gsd-core 1.20.1 against the live run's final report. The
  detection is removed once #6078 is fixed upstream.
- **D-23: parts of cut modules that kept code uses are relocated, not deleted** (D-01):
  - `parallel.rs`: `ensure_phase_worktree` (used by `start`, `commands.rs` L16) and `retry_after_from_reason`
    (`pipeline_outcomes.rs` L17) move before the module goes, together with their tests.
  - `ship.rs`: its reset-time parsing (`parse_rfc3339ish`, `hermes_schedule_from_retry_after`) is the starting point
    for D-27. Adapt it before deleting the file. `ship::shell_quote` has only cron callers, which are cut.
  - `lock.rs`: its callers (`recover.rs` L142 and L805, the stale-lock sweep; `run_checkout_hooks_from`,
    `pipeline_outcomes.rs` L1144) are removed with it. `Hook::BranchCreate` has no production constructor, so the
    planner checks whether the whole checkout-hook batch becomes dead code and removes it if so.
  - `ship_evidence::is_stopped_at` is used only by `evidence` (`commands.rs` L2036), which is cut.
- **D-24: Validate's GSD reader must never fail open** (D-08):
  - A missing `gsd-tools`, a nonzero exit, unparseable output or an unknown status fires a never-silent gate that
    names the failure. It is never a pass and never a silent retry.
  - The phase dir is resolved against the **worktree evidence root**, as the existing VERIFICATION lookups do
    (`pipeline_outcomes.rs` ~L515-530). Against `project_root` it would read `missing` and loop on execute-phase.
  - Each failure class gets a control.
- **D-25: gap rounds keep the consecutive-failure streak** (D-09, D-12). `transition_resets_consecutive_failures`
  (`mode.rs` L142-143) resets on every transition except Code→Validate. The new Validate→Plan(gaps)→Code path would
  therefore zero the streak on Plan→Code (`pipeline_gate.rs` L109-110), and `MAX_CONSECUTIVE_FAILURES` would never
  fire. The predicate must keep the streak across a gap-closure pass. Control: three gap rounds with no progress force
  the gate. The 10-per-phase ceiling stays as the backstop.
- **D-26: a carrier for the note** (D-14). `GateAction::LoopBack(Stage)` (`gates.rs` L85-108) carries no note, and
  every consumer drops it. The note must travel from the response to the relaunch prompt for every launch shape
  (claude stream, and the capture drivers codex and others), asserted per shape. `GateAction` is `pub`; the change is
  covered by the 4.0.0 major bump.
- **D-27: rate-limit gate mechanics** (D-15, D-16):
  1. `resetsAt` is free text today ("nothing parses this string", `agent_result.rs` L1778-1788). It must be parsed into
     a timestamp; a missing or invalid value means a plain gate.
  2. The waiting supervisor must not be ended before `resetsAt` + margin by `DEVFLOW_GATE_TIMEOUT_SECS` (default
     3 days, `config_parse.rs`; a weekly window can be longer) or by `gate sweep`. The wait bound is the later of the
     two, and sweep does not reap a rate-limit gate before its reset.
  3. "Auto relaunch used" is persisted in state, so a supervisor takeover, stop, resume or manual approve cannot
     re-arm it. A timer and an answer race through one durable claim, never two launches.
  4. Approve-to-relaunch needs its own action: `GateAction::Advance` means advance (`pipeline_outcomes.rs` L556,
     L742, L865).
  5. The gate context presents `devflow resume …` as the command for when the waiter is gone; while it is alive,
     resume is refused (`refuse_launch_over_a_live_run`, `commands.rs` ~L1010).

  Controls: reset before timeout, reset after timeout, a second denial, and restart.
- **D-28: abort-field schema** (D-13). `GateResponse` (`gates.rs` L54-63) gains the field with `#[serde(default)]`;
  absent means "not an abort". An answer that both approves and aborts is refused when it is written. `gate abort` is
  a `GateCmd` subcommand (`main.rs` L436), not a top-level `Command` variant; the planner checks what `doc_check`
  requires for it. Old 3.x records get no migration (operator rule 2026-10-12). Close-out checks the operator's own
  machine for pending 3.x answers.
- **D-29: what "finished" fires on** (D-06). `workflow_finished` is emitted at two sites: the `--until` halt
  (`pipeline_gate.rs` L88-96, `reason: stopped_at`) and the terminal finish. Notify fires for both, because both end the
  run, and the payload carries the reason. The terminal finish payload is `{"reason":"finished","shipped":false}`, a
  new shape that criterion 1 asserts.
- **D-30: kept modules' fixtures are rewritten** (D-03, criterion 2). Literals of the removed surface inside kept code
  fail the source scan: `history.rs` test fixtures (L263, L267), `tests/gitignore_coverage.rs` L38-39, `doc_check.rs`
  L323-331. They are rewritten, not allowlisted. State files written by 3.x that hold a removed `Stage` will no longer
  parse; there is no migration, and close-out clears the operator's own `.devflow/` of them.
- **D-31: D-02 ports a whole command, not just pins.** `release_check` (`commands.rs` from ~L5816) has five e2e tests
  (`tests/release_check.rs`) and depends on `version::read_workspace_self_pins`, which goes with `version.rs`. The
  port covers the command's checks (pins, main divergence, absent-origin fallback, publish order) and keeps a
  negative control for each.

- **D-32 (operator, after check 5): `stale` routes get an explicit prompt and a post-check** (amends D-08, D-10).
  Live check 5 showed the execute-phase agent can skip the required re-verify: it read `passed` from the frontmatter and
  judged the change test-only (open-gsd/gsd-core#6096). So:
  1. The relaunch prompt quotes GSD's status and route verbatim: "verification is stale … resume at the phase gates
     and re-run the verifier (#4682)".
  2. When Code ends, DevFlow re-reads `verification status`. If it is still `stale`, that Code pass counts as failed
     and fires a gate, never a pass.

  This makes a skipped re-verify visible; it does not prevent one. Control: a Code pass that leaves the report
  `stale` ends at a gate, not at Validate's "passed".
- **D-33 (operator, after check 5): uncovered changes count as stale** (amends D-10, D-21). A fix that only touches
  files outside the report's `covered_files` leaves GSD's status `passed` (check 5 probe). After a clean-reject fix,
  DevFlow lists the files the fix commits changed (git), minus `.planning/`, and subtracts GSD's `covered_files` (read
  with `gsd query frontmatter get`). Anything left is treated as `stale`: the D-32 re-verify relaunch and post-check
  run. This is a stopgap beside GSD's own rule; the upstream request is on #6096 (`/gsd-quick --phase N`), and the
  check is removed once GSD links quick tasks to a phase. Controls: a fix to an uncovered new file triggers the
  re-verify; a fix touching only `.planning/` does not.

### Decisions after research (operator, 2026-11-09)

Source: the three open questions in `52-RESEARCH.md`, each checked against the code before it was put to the operator.

- **D-34: only commits outside `.planning/` count as progress** (amends D-25). `consecutive_failures_made_progress`
  (`mode.rs` L180-181, used at `pipeline_outcomes.rs` ~L610) treats any new commit on the phase branch as progress and
  resets the streak to 1. Every gap round commits `.planning/` files (the gap plan, summaries, the re-verified report),
  so the 3-in-a-row cap could never fire on a real gap loop. Progress is now commits that change at least one path
  outside `.planning/`. This changes the 999.66 guard, and a docs-only Code fix also counts as no progress. Control:
  three gap rounds that commit only `.planning/` files force the gate. One round that also changes a source file
  resets the streak.
- **D-35: finishing keeps the run's state** (amends D-04, D-29). Today the finish path deletes state
  (`clear_run_state`, `pipeline_gate.rs` ~L355). That suited 3.0, where Ship merged and deleted the branch. In v4 the
  branch and worktree stay, and `cleanup` removes a worktree with no state without `--force` (it keeps only a
  `stopped` state, `commands.rs` ~L1330-1347). So the state stays, recorded as stopped with reason `finished`:
  - `status` shows the run as finished;
  - `cleanup` keeps the worktree and says why, and `cleanup --force` removes both (OPS-02);
  - a new `start` on the phase says it replaces the finished state;
  - the phase is still deregistered from the machine-global gate registry.

  Control: after a finished run, `cleanup` without `--force` keeps the worktree.
- **D-36: on a `gaps_found` Validate, `gate approve` runs the gap loop** (amends D-04 for that case). In supervise
  mode, the Validate gate on a gaps verdict offers three answers:
  - approve: close the gaps (D-09: Plan `--gaps` → Code `--gaps-only` → Validate);
  - reject with a note: steer that fix;
  - abort: end the run.

  Finishing over open gaps is not offered, so a run never ends `finished` with a gaps report. Control: approving a
  gaps gate never records `workflow_finished`.

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
- 999.206 is a **write-time** cap: `truncate_reason` = `render_gate_context(reason, 300)`
  (`crates/devflow-cli/src/pipeline_outcomes.rs` L888, called at L553 and L923). The display constant
  `GATE_CONTEXT_DISPLAY_CHARS` (`commands.rs` L2056) is a separate 300. `gate_show` already prints the stored context
  untruncated (test `gate_show_renders_full_untruncated_sanitized_context`, `commands.rs` L10062). The fix therefore
  belongs at write time, and criterion 4 needs a control: a reason longer than 300 chars survives to `gate show`.
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
  Filed 2026-11-08 as https://github.com/open-gsd/gsd-core/issues/6078 (D-11), with a reproduction and a `passed` control.

</deferred>

---

*Phase: 52-subtract-and-stabilize*
*Context gathered: 2026-11-08*
