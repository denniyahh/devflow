# Phase 52: Subtract and Stabilize - Research

**Researched:** 2026-11-09
**Domain:** Rust workspace subtraction (published `devflow-core` 4.0.0 major) + supervise gate-loop and operator-surface fixes; GSD (gsd-core 1.20.1) reader integration
**Confidence:** HIGH for the code graph and anchors (read this session); MEDIUM for GSD integration (probed live on 1.20.1, n=1 per shape); LOW only where tagged `[ASSUMED]`

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

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

### Deferred Ideas (OUT OF SCOPE)
- **Phase 54:** fold `auto`/`supervise` into a triage policy, where "supervise" = the operator answers everything,
  including the finish review, and "auto" = triage decides. Consider this in Phase 54's discuss step, once the model
  exists (operator, 2026-11-08).
- **Upstream (gsd-core):** `readVerificationStatus` returns `gaps_found` before the staleness check
  (`verification.cjs` L1537), so a gaps report whose covered files changed cannot be routed to re-verification.
  Filed 2026-11-08 as https://github.com/open-gsd/gsd-core/issues/6078 (D-11), with a reproduction and a `passed` control.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description (short) | Research Support |
|----|---------------------|------------------|
| CUT-01 | No Define; start at Plan; dry run shows no Define | §Cut graph rows "Define"; §Hidden Define couplings (`State::new`, `run_never_prepared`, interactivity, doctor) |
| CUT-02 | Run ends after Validate as finished; no Ship/hooks/`ship`/`--yes-ship`; `workflow_finished` `shipped:false` | §Cut graph Ship rows; §Deletion order plans 05a/05b; §GATE/OPS anchors "finish" |
| CUT-03 | Rate limit with no fallback waits at a gate; one auto relaunch at reset; no cron file | §Rate-limit gate design; §Cut graph `ship.rs` relocation; criterion-3 fixture `failover_e2e.rs:839` |
| CUT-04 | All 28 cut rows removed with docs/tests/allowlist; `check.sh all` green | §Cut graph (all rows); §doc_check mechanics; §Source-scan test design |
| GATE-01 | Note verbatim in relaunch prompt; abort only on explicit answer | §Gate answers anchors; §Note carrier pattern |
| GATE-02 | Validate rejection runs only fix work (gaps route / `/gsd-quick`) | §Validate verdict anchors; §GSD integration |
| GATE-03 | `gaps_found` driven to `passed` in-run; gate never says passed over gaps | §GSD integration (probed statuses); §D-25 streak pitfall |
| GATE-04 | `gate show` full reason; way-on sentence first | §Write-time cap anchors |
| OPS-01 | `--version` shows commit + dirty | §OPS anchors (`build.rs` L60/L69, `main.rs` `#[command(version)]`) |
| OPS-02 | cleanup says why it keeps; `--force` removes state; status never shows gone worktree as active; start says it replaces stale state | §OPS anchors (`commands.rs` L1300-1418, L1862) |
| OPS-03 | No tracing unless `RUST_LOG`; doctor RUST_LOG info; no provenance warning off-repo; one missing-remote line | §OPS anchors (`main.rs` L580/L589, `staleness.rs` L497-503, `preflight.rs` L381-386/L574) + conflicting test `log_format_env.rs:167` |
| OPS-04 | status shows stop reason + fallback state | §OPS anchors (`commands.rs` L1862-1960) |
</phase_requirements>

## Project Constraints (from CLAUDE.md)

- Crates: `devflow-core` (lib, **published**: removing/changing a `pub` item is semver; run `cargo semver-checks check-release -p devflow-core`), `devflow-cli` (package name **`devflow`**, binary only — `cargo test -p devflow --lib` verifies nothing).
- Gate each task on ONE named test per command: `cargo nextest run -p <pkg> --lib --no-tests=fail -E 'test(=module::path::name)'` (`--bin devflow` for CLI unit tests; `--test <file>` for integration tests). Chain with `&&`; never `test(=a) | test(=b)`.
- `rg -c` prints nothing on zero matches: "no X left" checks must be Rust source-scan tests (doc_check pattern), not shell counts. Prefer named Rust tests over shell pipelines.
- Every literal `std::env::var("DEVFLOW_…"` / `var_os(` / `env_value(` under `crates/` (tests too) and every `Command` variant must be in README/ARCHITECTURE/CONTRIBUTING/OPERATIONS.md or `docs/guides/`, or allowlisted, in the same change.
- `build.rs` embeds commit + dirty flag: never compare a rebuilt binary's sha256 to a stored one.
- `core.fileMode=false`: `git add <f> && git update-index --chmod=+x <f>`.
- Work in `.worktrees/phase-52`; no concurrent git ops on the main checkout. `graphify-out/` never committed.
- `.planning/user/DEV-SETUP-CHECKLIST.md` updated in the same commit when CI workflows, hooks, devcontainers or tooling deps change (relevant if the release tool or a new dev tool lands).
- Dogfooding rule: Phase 52 is the recorded exception (D-18). Dry-run control uses the installed binary: `devflow --version` printed `devflow 3.0.0` this session `[VERIFIED: shell]`.
- Global operator rules that bind plans: no invented constraints recorded as operator decisions; never-silent fail-closed diagnostics; kill only by recorded PID; scratch under the session scratchpad; avoid shell-check gates.

## Summary

Phase 52 is ~80% subtraction. The removed surface is not isolated: `Stage::Ship` has **~300 references** and `Stage::Define` **~170** across both crates (counted `rg -c 'Stage::Ship'` / `'Stage::Define'`, per-file totals summed), several cut modules feed kept code (`supervisor.rs:509` calls `crate::ship::civil_from_days` — **not in D-23**), the e2e harness uses `--legacy-claude-launch` purely to get the capture launch shape (10 test files), and `doc_check` already fails in both directions on any doc/source mismatch, so docs must change in the same plan as each removal (not in a trailing docs plan). The safe order is: independent leaf commands first, then relocations, then make Validate terminal while `Stage::Ship` still exists but is unreachable, then delete Ship-only modules, then remove the variants, then the source-scan test and the 4.0.0 bump.

The GATE/OPS work has three integration hazards the CONTEXT does not fully settle: (1) GSD readers return **exit 0** for `phase_dir_not_found` and for a missing frontmatter field (`{"error":"Field not found"}`) — a fail-closed parser must key on the JSON, not the exit code `[VERIFIED: live probe gsd-core 1.20.1]`; (2) the CI image (`mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm`) almost certainly has no `node`, so every test must use a stubbed GSD reader `[ASSUMED]`; (3) D-25's "three gap rounds with no progress" cannot fire with only the transition-predicate fix, because `consecutive_failures_made_progress` treats *any* new commit on the phase branch as progress and `/gsd-plan-phase --gaps` always commits a plan `[VERIFIED: agent_result.rs:2858-2898, mode.rs:150-175]`.

**Primary recommendation:** Run Wave 1 as a strictly serial chain of 7-8 compile-green subtraction plans (each carries its own docs, allowlist and test edits and gates on the named doc_check tests), then build one `gsd_reader` module in `devflow-core` behind a stub-able launcher (`$HOME/.claude/gsd-core/bin/gsd_run`, a POSIX `sh` script) before any Validate wiring.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Stage machine (Plan→Code→Validate, finish) | `devflow-core` (`stage.rs`, `mode.rs`, `state.rs`) | CLI `pipeline_gate.rs::transition` | Enum + policy are library-owned; CLI only drives |
| Validate verdict pairing (agent AND GSD status) | CLI `pipeline_outcomes.rs::handle_validate_outcome` | new core `gsd_reader` | Routing is pipeline policy; reading GSD is a reusable, unit-testable seam |
| GSD reader invocation (status/frontmatter/fingerprint) | new `devflow-core::gsd_reader` | — | Deterministic subprocess + JSON parse; must be fail-closed and stub-able |
| Gate answers (abort field, note) | `devflow-core::gates` | CLI `commands.rs::gate_respond`, `main.rs::GateCmd` | Schema lives with the gate protocol |
| Note → prompt | `devflow-core::prompt` (`StageIntent`, `fix_prompt`, `render_workflow_style`) | every driver in `agents/` | One renderer per style, so all shapes get it |
| Rate-limit wait + one auto relaunch | CLI `pipeline_outcomes.rs` / `pipeline_gate.rs` | core `gates.rs` (answer/claim), `state.rs` (used flag) | Reuses the existing first-writer-wins answer + claim protocol |
| Reset-time parsing | new core module (relocated from `ship.rs`) | `supervisor.rs::rfc3339_from_unix` | Pure functions; shared by the gate and supervisor timestamps |
| Release preflight (port of `release --check/--verify`) | repo tooling (outside the product binary) | `scripts/cut-release.sh` | D-02: not part of the runner |
| Operator surface (`--version`, cleanup, status, doctor, tracing) | CLI `main.rs`, `commands.rs`, `staleness.rs`, `preflight.rs` | `build.rs` | Pure presentation |

## Standard Stack

No new external crates. The phase removes two.

| Item | Version | Purpose | Status |
|------|---------|---------|--------|
| clap (derive) | workspace `4` | CLI; removing variants makes cut verbs "unrecognized subcommand" automatically | keep `[VERIFIED: Cargo.toml]` |
| serde / serde_json | workspace `1` | `#[serde(default)]` for the abort field, GSD JSON parse | keep |
| tracing-subscriber (`env-filter`) | workspace | OPS-03 default filter change | keep |
| `semver` `1`, `git-conventional` `1` | devflow-core deps | used **only** by `version.rs` (26 refs, all in that file) | **remove** with `version.rs`, or `cargo machete` (`scripts/check.sh deps`) flags them `[VERIFIED: rg 'semver::|git_conventional']` |
| gsd-core | 1.20.1 | `query verification status`, `query frontmatter get`, `query verification fingerprint` | external runtime dep, see §GSD integration `[VERIFIED: runtime-identity --raw]` |
| cargo-nextest | 0.9.151 (host) | named-test gates | `[VERIFIED: shell]` |
| cargo-semver-checks | 0.51.0 (host); not in `check.sh` or CI | `check-release -p devflow-core` (`--release-type`, `--baseline-version` flags exist) | `[VERIFIED: --help]` |

**Installation:** none.

## Package Legitimacy Audit

No packages are installed by this phase. Packages removed: `semver`, `git-conventional` (devflow-core). If the planner puts the release tool in its own crate and it needs semver parsing, reuse `semver = "1"` (already in `Cargo.lock`; no new name enters the tree).

| Package | Registry | Verdict | Disposition |
|---------|----------|---------|-------------|
| (none added) | — | — | — |

**Packages removed due to [SLOP] verdict:** none. **Packages flagged [SUS]:** none.

## The Cut's Dependency Graph (CUT-01..04)

Legend: **D** delete · **R** relocate (keep, move) · **W** rewrite kept code/test · **S** semver (pub item in published core). Line numbers are this tree (`f3c8600`), read this session.

### Commands / flags (CLI)

| Cut row | Defined at | Kept code/tests/docs that reference it | Action |
|---|---|---|---|
| `reference` | `main.rs` `Reference` variant; `commands.rs:1170` `reference()` | `cleanup` reads `worktree::reference_path` (`commands.rs:1306`, `:1316` "keeping reference worktree"); `worktree.rs:52` `pub fn reference_path` (S), `worktree.rs:239`; test `phase7_cli.rs:836` `reference_and_cleanup_worktree_cli_flow` | D verb + `reference_path` (S); W cleanup (drop reference branch); rewrite the phase7 test to cover cleanup only |
| `list` | `main.rs` `List`; `commands.rs:5278` | `git.rs:454` `list_feature_branches` (only caller `commands.rs:5280`) + unit test `git.rs:922` | D verb; D `list_feature_branches` + its test (S) |
| `test` | `main.rs` `Test`; `commands.rs:5511` `test_cmd` | `ci_parity_guards.rs:388-403` `devflow_test_clippy_matches_ci_scope` (+ module doc L30) | D verb + that test |
| `release --check/--verify` | `main.rs` `Release` + dispatch (`main.rs:781-804`); `commands.rs:5816` `release_check`, `:6187` `release_verify`, helpers `check_self_pin:5929`, `check_divergence:5993`, `check_publish_order:6024`, `check_changelog_version:6048`, `compare_versions:6145`, `check_tag_on_main:6227`, `check_sync_done:6297` | `scripts/cut-release.sh:5,13,22,88,335`; `tests/release_check.rs` (6 tests: L86, L104, L125, L152, L186, L211); `version::read_workspace_self_pins` (`commands.rs:5945,6063,6242`) | Port 6 checks (4 `--check` + 2 `--verify`) **before** `version.rs` goes (D-02/D-31); see §Release port |
| `evidence` | `main.rs:413-431` (variant at L418); `commands.rs:1995` | `ship_evidence.rs` (only caller `commands.rs:42,2001,2036`); Ship-finalization tests in `pipeline_gate.rs:1158,1205,1212,2479` | D with the Ship plan (its tests die with Ship) |
| `ship --phase [--force]` | `main.rs` `Ship`; `pipeline_gate.rs:818` `ship_override`, `:883` `claim_ship_answer` | `commands.rs:866` `ship_answer_stored`, `:4772` `refuse_ship_over_a_live_supervisor`; repair hints naming `devflow ship`: `commands.rs:1658,1673,2471,2488,4281,4377,4786,6453`, `pipeline_launch.rs:3490`; `stop` Ship special case `commands.rs:4049` | D; W every hint (source scan should list `devflow ship` as a forbidden literal) |
| `--yes-ship` / `DEVFLOW_YES_SHIP` / toml `yes_ship` | `main.rs` start `yes_ship`; `config.rs:174,279` + `Default` L149; `state.rs:354` `pub yes_ship` (S) | `print_dry_run` "ship gate:" block `pipeline_gate.rs:1016-1031`; tests `yes_ship_config.rs` (whole file); `CLAUDE.md:37` prose mentions `yes_ship` (operator file, not in scan scope) | D all; delete `yes_ship_config.rs` |
| `--legacy-claude-launch` (start, resume) / `DEVFLOW_CLAUDE_LEGACY_LAUNCH` / legacy path | `main.rs:112` (start), `main.rs:216` (resume); `config.rs:294-320`; `state.rs:408` `pub legacy_claude_launch` (S); `pipeline_launch.rs:187,309-357,440,655,894-898,3577-3657,3710-3751` | **e2e harness uses it to get the capture shape:** `supervisor_e2e.rs:81,96,753`, `pending_supervisor_e2e.rs:168`, `gate_notify_persist_e2e.rs:202,402`, `start_lock_e2e.rs:76,284,1031`, `test_home_isolation.rs:82`, `gate_wedge_e2e.rs:178`, `phase7_cli.rs:148`; unit tests set `state.legacy_claude_launch = true` "because its subject is orthogonal to the launch path" at `pipeline_launch.rs:4486,4608,5057,5116,5501,5652,5726,5827,5933,6155,6213` | D flag/env/field/`LegacyLaunchSource`/`apply_legacy_launch_opt_out`/`forced_legacy_launch_notice`; `stream_launch_enabled` loses its third parameter. **Keep `MonitorLaunch::Legacy`** (`monitor.rs:306-320`) — it is the *capture shape* for codex/opencode/pi/hermes and `relaunch_checkpoint_session`, not the legacy Claude launch. W tests to use a capture-shape agent (`common::fake_bin_dir_for("codex", …)`, `tests/common/mod.rs:110`) |
| hidden `start --worktree` | `main.rs:64-65` + `worktree: _worktree` in dispatch | `phase7_cli.rs` literal | D |
| `gate approve/reject` positional `STAGE_OR_PROJECT`/`PROJECT` | `main.rs:454-458, 481-485`; `commands.rs:53` `resolve_gate_target` | dispatch `main.rs` gate arms | D positional args; `resolve_gate_target` collapses to `--stage` only |
| `gate sweep --reap-strays` | `main.rs` Sweep `reap_strays`; `commands.rs:2512-2660` (`gate_sweep` branch), `:2786` `StrayReapOutcome`, `:2815`, `:2846` `reap_stray_candidates` | doctor stray report **stays** but its repair hints name the cut flag: `commands.rs:2667`, `:7522` (tests `:13469,:13510,:13668`); `tests/reap_strays_e2e.rs` (whole file) | D flag + reaping code + e2e file; W doctor hint (e.g. name the pid and `kill <pid>` after `ps -o args= -p <pid>`) — Claude's discretion |
| `parallel` | `main.rs:22-23,265-281,753-765`; `parallel.rs` | `commands.rs:16,658,16972` use `ensure_phase_worktree`; `pipeline_outcomes.rs:17,96` use `retry_after_from_reason`; tests `phase7_cli.rs` `parallel_creates_two_worktrees_and_spawns_two_monitors`, `fallback_agent_config.rs` `parallel_ignores_the_config_key`; `parallel.rs` unit tests L230 `ensure_phase_worktree_forks_from_the_supplied_base`, L280 `retry_after_from_reason_strips_prefix` | R both helpers + their two unit tests (e.g. `ensure_phase_worktree` → `commands.rs` or a new `worktree_cli.rs`; `retry_after_from_reason` → the new reset-time module or `pipeline_outcomes.rs`); D the rest |

### Env vars

| Var | Read at | Action |
|---|---|---|
| `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` | `config_parse.rs:69-70` (`foreground_gate_timeout_secs`, only for `ship_override`) | D with Ship + its OPERATIONS row |
| `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` | `config_parse.rs:85-86` → `pipeline_outcomes.rs:1144` | D with hooks/lock |
| `DEVFLOW_REVIEW_ANGLES` | `config.rs:240` `review_angles` (+ `DevflowConfig::review_angles` L164, toml key) → `prompt.rs` `StageIntent::for_stage_in_project` Ship arm, `SHIP_REVIEW_ANGLES` | D with `StageIntent::Ship` (S) |
| `DEVFLOW_YES_SHIP`, `DEVFLOW_CLAUDE_LEGACY_LAUNCH` | above | D |

### Stages, hooks, background

| Cut row | Defined at | Kept references | Action |
|---|---|---|---|
| Define stage | `stage.rs:18` (+ `next`, `is_agent_stage`, `gsd_command`, `Display`, `FromStr`, `StageParseError` text L101) (S); `prompt.rs` `StageIntent::Define`, `define_stage_prompt` | **Hidden couplings:** `State::new` starts at `Stage::Define` (`state.rs:709-712`); `run_never_prepared` keys on `state.stage == Stage::Define` (`commands.rs:889`); start's interactivity probe `driver.interactivity_mode(Stage::Define)` (`commands.rs:600`); doctor `check_missing_branch` `facts.stage == Stage::Define` (`commands.rs:6681`); `print_dry_run` starts `Some(Stage::Define)` (`pipeline_gate.rs:948`); `outcome_policy.rs`, `agents/mod.rs`, `agents/codex.rs`, `preflight.rs` (20), `pipeline_launch.rs` (45), `commands.rs` (27); e2e stubs answer `stage define` turns and some assert invocation sequences (`failover_e2e.rs` "Define, Plan, Code and the handoff", `gate_wedge_e2e.rs` 9, `stop_e2e.rs` 7, `phase7_cli.rs` 7, `doctor_json_e2e.rs:71-79` opens a Define gate) | Re-key every "not yet launched" test to Plan (behaviour change — each needs a test); remove variant last |
| Ship stage | `stage.rs:26` (S); `StageIntent::Ship`, `ship_stage_prompt`, `workflow_ship_prompt` (`prompt.rs:594-627`), `ship_pr_base_instruction` | Validate→Ship call sites `pipeline_outcomes.rs:556,744,762`; `pipeline_launch.rs:4003` `Stage::Ship => handle_ship_outcome`; `pipeline_launch.rs:4194` Ship failure arm; `mode.rs:250` `Stage::Ship => true` in `should_gate`; `main.rs:628` `--until ship` refusal | Make Validate terminal first (new `finish_run`), then remove |
| `DocsUpdate` + after-ship `Merge`/`VersionBump`/`ChangelogAppend`/`BranchCleanup` | `hooks.rs` (whole module, `pub mod hooks`) (S) | `hooks_for_transition` returns non-empty **only** for `(Validate, Ship)` (`hooks.rs:100-105`); `Hook::BranchCreate` has no production constructor → after the cut the **entire checkout-hook batch is dead**: `pipeline_gate.rs:101-106` (`run_checkout_hooks` call in `transition`), `pipeline_outcomes.rs:1089` `hook_context_root`, `:1105` `run_checkout_hooks`, `:1126` `BatchProgress`, `:1134` `run_checkout_hooks_from` | D `hooks.rs` entirely (answers D-23's question: yes, dead) |
| Finalization retry gate | `pipeline_gate.rs:238-388` (`finish_workflow`, `finish_workflow_with_gate_timeout`) | `pipeline_outcomes.rs:865`, tests `pipeline_gate.rs:1278,1429` | D; the new terminal finish is small (see §Patterns) |
| `gh auth` / major-bump preflight | `preflight.rs:694-760` (`gh_auth_check_applies`, `preflight_gh_auth_check`), `:761-863` (`major_bump_check_applies`, `preflight_major_bump_check`), `:864` `breaking_commit_subjects` | callers in `generic_preflight_checks` `preflight.rs:1330`; tests `preflight.rs:2172` etc. | D |
| `lock.rs` | `pub mod lock` (S) | `recover.rs:142-148`, `:805-811` (`remove_stale_locks_with_outcomes`), `doc_check.rs:6` import + `:323`; `pipeline_outcomes.rs:1144,1274`; `plan_bashism_scanner.rs:554,672` (string literals of test names only — fixture text, not code refs) | D; W recover sweep (drop the lock arm); W doc_check |
| `version.rs` | `pub mod version` (S) | `hooks.rs` (dies), `preflight.rs:767-853` (dies), `commands.rs:46,5945,6063,6242` (release port), `pipeline_gate.rs:1334-1407` tests (die with Ship), `ship.rs:513` doc link; `agent_result.rs:10266` comment | D after release port |
| `ship.rs` (incl. cron file) | `pub mod ship` (S) | **R:** `civil_from_days` (`ship.rs:472`) is called by **kept** `supervisor.rs:509` `rfc3339_from_unix`; retry parsing `parse_retry_timestamp`/`parse_unix_seconds`/`parse_rfc3339ish`/`split_time_and_offset`/`parse_offset_minutes`/`days_from_civil`/`RetryTimestamp` (`ship.rs:304-490`) → reset-time module for D-27. **D:** cron writer/reader/consumer: `pipeline_outcomes.rs:104-147` (writer + `rate_limit_resume_scheduled` event), `pipeline_gate.rs:360-379` and `pipeline_launch.rs:2329-2347` (`consume_cron_instructions`), `commands.rs:1955,5185-5233` (status hints), `recover.rs:228-270,786-803` (cleanup), doctor/recover report field `orphan_cron_records_removed` (`commands.rs:5394-5434`); `prepend_changelog` (hooks only); `shell_quote` (cron only). Tests: `recover.rs:952-957,1328-1355,1810-1846,2793-2833`; `pipeline_outcomes.rs:5081,5131,5172,5198,6104,6121`; `pipeline_launch.rs:5774-5791,6097-6113,6194-6238`; `commands.rs:10124-10218`; `recover_clean_e2e.rs:436-468` (`sweep_removes_and_names_an_orphan_cron_record`); `phase7_cli.rs:806-825`; `failover_e2e.rs:410-411,509,565,839-891`; core `tests/devflow_dir_gitignore.rs:26,173-184`; `doc_check.rs:325-330,784-842` (an OPERATIONS row pinned as `"| \`cron-instructions-NN.json\`"`); `gitignore_coverage.rs:37-39` | R first, then D |
| `ship_evidence.rs` | `pub mod ship_evidence` (S) | `commands.rs:42,1962-2040`; `events.rs:147,414` (comments); `main.rs:417` doc | D with `evidence` |

### Kept-code literals the criterion-2 scan will hit (rewrite, not allowlist — D-30, extended)

D-30 lists `history.rs:263,267`, `gitignore_coverage.rs:38-39`, `doc_check.rs:323-331`. Also found this session:
- `events.rs:224,238,313,431,437,442,480` — `"stage": "ship"` / `"to": "ship"` test fixtures `[VERIFIED: rg]`.
- `history.rs:269,303` — `"stage":"ship"` capture fixture and `gate_fired (ship)` assertion.
- `gitignore_coverage.rs:37` — `.devflow/last-ship.json`.
- `doc_check.rs:784-842` — `operations_md_documents_the_fallback_agent_and_its_events` pins a `cron-instructions-NN.json` OPERATIONS row.
- `log_format_env.rs:169-171` — opens a `Stage::Ship` gate.
- `.gitignore:51,57,58` — `lock-*`, `last-ship*.json`, `cron-instructions*.json` (not in scan scope; remove with the gitignore_covers test rewrite).
- `devflow.toml:2-3` comment "Ship merges back into" (not in scan scope; reword).
- `scripts/hooks/pre-commit:22-25` allowance "for automated DevFlow terminal hooks (e.g. VersionBump, ChangelogAppend)" via `DEVFLOW_TERMINAL_HOOK` — no Rust code sets that variable (`rg` over the repo finds only the hook). Out of the criterion's scan scope; repo-hook rows are `defer` in the inventory, so mention, don't remove.

### doc_check mechanics the planner must design around `[VERIFIED: doc_check.rs:358-433]`

- **docs→source** (`doc_referenced_identifiers_exist_in_source`): every `DEVFLOW_*` token, every `devflow <word>` (checked against `Command` **and** `GateCmd` variants, lowercased), every `--flag` (passes if the literal or `field_name:` exists in source), every documented Rust identifier and `.devflow/` path must exist in source. So **deleting a verb or env var while a doc still names it fails `cargo test -p devflow-core --lib`** — docs must change in the same plan.
- **source→docs** (`source_devflow_env_vars_and_subcommands_are_documented`): every env literal read via `std::env::var(`/`var_os(`/`env_value(` and every **`Command`** variant must appear in scoped docs. `GateCmd` variants are **not** checked source→docs, so `gate abort` needs no allowlist entry to pass; document it in OPERATIONS "Answering gates" anyway (answers D-28's question).
- Scoped docs = `README.md`, `ARCHITECTURE.md`, `CONTRIBUTING.md`, `OPERATIONS.md` + `docs/guides/*.md` (`doc_check.rs:8-13,55-67`). `docs/architecture/*`, `docs/diagrams/*`, `CHANGELOG.md` are **not** scoped. `docs/architecture/git-ship.md` and `docs/diagrams/ship-flow.md` are whole-Ship docs (delete; fix `docs/mkdocs.yml` nav and `docs/index.md`).
- Allowlist entries tied to cut docs: `--no-deps` ("cargo doc option described in architecture" — only `ARCHITECTURE.md:66`, the `DocsUpdate` row) and `--no-ff` (`ARCHITECTURE.md:388`, merge-hook prose). Remove both entries with those doc lines. No other allowlist entry references cut surface `[VERIFIED: doc-check-allowlist.toml read in full]`.
- Help snapshot `tests/snapshots/devflow-help.txt` lists `parallel`, `reference`, `list`, `test`, `release`, `ship`, `evidence` and "Define → … → Ship" — regenerate in each plan that changes the top-level surface (`help_snapshot.rs:26` `help_output_matches_committed_snapshot`).

### Safe deletion order (Wave 1, strictly serial — the plans share `main.rs`, `commands.rs`, `pipeline_*.rs`)

Each plan ends compiling, with `cargo nextest run -p devflow-core --lib --no-tests=fail -E 'test(=doc_check::doc_referenced_identifiers_exist_in_source)'` and `…'test(=doc_check::source_devflow_env_vars_and_subcommands_are_documented)'` and the help snapshot test green.

| # | Plan | Why this position |
|---|------|-------------------|
| 1 | Leaf verbs/flags: `reference`(+`reference_path`), `list`(+`list_feature_branches`), `test`, gate positional args, hidden `--worktree`, `--reap-strays`(+doctor hint rewrite) | No Stage coupling; smallest diffs |
| 2 | Legacy Claude launch: flag, env, `State.legacy_claude_launch`, opt-out predicate; migrate e2e harness to a capture-shape agent | Big test churn; isolate it so later failures are attributable |
| 3 | Release port (D-02/D-31) + remove `release` verb; update `cut-release.sh` | Must precede `version.rs` deletion |
| 4 | `parallel` cut with relocation of `ensure_phase_worktree`, `retry_after_from_reason` (+ their unit tests) | Must precede `ship.rs`/rate-limit rework that imports the helper |
| 5a | Make Validate terminal: new `finish_run` (`workflow_finished {"reason":"finished","shipped":false}`, worktree/branch kept, notify once — D-06/D-29), replace the three `transition(…, Stage::Ship)` sites; rate-limit-without-fallback becomes an interim plain blocking gate (no cron); relocate retry-time parsing and `civil_from_days` out of `ship.rs` | `Stage::Ship` still compiles but is unreachable; behaviour is now v4 |
| 5b | Delete Ship-only code: `devflow ship`, `finish_workflow*`, `hooks.rs`, `lock.rs`, `version.rs` (+deps), `ship.rs`, `ship_evidence.rs` + `evidence`, gh-auth/major-bump preflight, `--yes-ship`/`DEVFLOW_YES_SHIP`/`yes_ship`, `DEVFLOW_REVIEW_ANGLES`, `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS`, `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS`, then the `Stage::Ship` and `StageIntent::Ship` variants | One coherent deletion; may need splitting by file if the executor's context is tight (memory: gsd-planner/executor degrade past ~200k tokens) |
| 6 | Define cut: `State::new` at Plan, dry run from Plan, re-key `run_never_prepared`/interactivity/doctor, delete `define_stage_prompt`, `Stage::Define`, `StageIntent::Define` | After Ship so `Stage` loses both variants in two reviewable steps |
| 7 | Criterion-2 source-scan test + planted control, whole-doc deletions (`git-ship.md`, `ship-flow.md`), `.gitignore`, version bump to 4.0.0 (two places: `[workspace.package] version` and the `devflow-core` self-pin, `Cargo.toml:9,20`), `cargo semver-checks check-release -p devflow-core` | Last, so the scan runs on the finished cut |

### Release port (D-02, D-31) — recommendation

`release_check` runs 4 checks (`check_self_pin`, `check_divergence`, `check_publish_order`, `check_changelog_version`, `commands.rs:5816-5822`) and `release_verify` 2 (`check_tag_on_main`, `check_sync_done`, `commands.rs:6187-6191`). Tests: 6 in `tests/release_check.rs` (L86, L104, L125, L152, L186, L211; L125 tests the bare-`release` refusal and dies with the verb). The self-pin check already has two independent guards in the normal suite: `tests/workspace_version_pin.rs::workspace_member_pins_match_the_workspace_version` and `doc_check::devflow_core_dependency_pin_matches_workspace_version` (`doc_check.rs:494`).

**Recommend a selftested Rust tool, not shell** (operator rule "avoid shell-check gates"; CLAUDE.md "prefer a named Rust test to any shell pipeline"): an unpublished workspace binary (e.g. `tools/release-preflight`, `publish = false`) that carries the 5 surviving checks and the 5 surviving e2e tests (pins, divergence, absent-origin fallback, publish order, plus changelog/tag/sync), invoked by `cut-release.sh:88,335` via `cargo run -q -p <tool> -- check|verify`. Cost to flag: a new workspace member changes `check_publish_order`'s own premise (it lists publishable crates — the tool must be `publish = false` and excluded from that list), widens `cargo clippy --workspace`, and needs a `DEV-SETUP-CHECKLIST.md` line. `[ASSUMED: tool layout]` — the planner must state the choice and reason (D-02).

## GATE / OPS Code Paths (verified anchors)

| Concern | Anchor | Current behaviour | Change |
|---|---|---|---|
| Abort by substring | `gates.rs:94-107` `GateAction::from_response` | `note.to_ascii_lowercase().contains("abort")` → `Abort` | Read an explicit `abort` field only (D-13) |
| `GateResponse` schema | `gates.rs:52-63`: `pub approved: bool`, `#[serde(default)] pub note: Option<String>`, `#[serde(default)] pub responded_by: Option<String>` | No abort field; flattened into `GateAnswer` (`gates.rs:66-74`, `#[serde(flatten)] pub response`) | Add `#[serde(default)] pub abort: bool`; refuse approve+abort at write (`Gates::answer` path, `commands.rs:2241` `gate_respond`). `GateResponse` is not `#[non_exhaustive]` → adding a pub field breaks struct literals (S, accepted under 4.0.0); consider adding `#[non_exhaustive]` in the same change, as `State` and `FixType` did |
| `GateAction` | `gates.rs:84-90`: `Advance`, `LoopBack(Stage)`, `Abort(String)` | Note dropped | Carry the note (e.g. `LoopBack { stage, note }`) and add the relaunch action D-27.4 requires. Consumers: `pipeline_outcomes.rs:557,746,866,962`, `pipeline_gate.rs:316,692,861`, `preflight.rs:1529`, `commands.rs:4164`, `pipeline_launch.rs:4124`, `mode.rs` (1), `gates.rs` tests |
| Writers of "abort" notes | `commands.rs:2741-2747` (sweep: `"abort: reaped by devflow gate sweep …"`), `commands.rs:4058-4064` (stop: `"abort: stopped by \`devflow stop\`"`), both via `Gates::reap_answer` (`gates.rs:680-695`) | Rely on the substring | `reap_answer` sets `abort: true`; its doc (`gates.rs:672-679`) changes; `stop_with_existing_response` (`commands.rs:4150-4169`) matches the new field |
| Note never reaches prompt | `prompt.rs:713-741` `fix_prompt(fix_type, phase)`; `StageIntent::Code { phase, fix }` (`prompt.rs:166-184`); `render_claude_style` `:479-495`; `render_workflow_style` `:506-519` → `workflow_code_prompt` `:558-585`; `loop_back_to_code`/`prepare_loop_back_to_code` (`pipeline_gate.rs:167-235`) | Note discarded | Note field on `StageIntent::Code` (or a quick-fix intent), rendered by **both** renderers. Persist it beside `State.code_fix` (`state.rs:507`, `Option<FixType>`) so a failover/resume replay of the fix pass keeps it (Phase 51 D-09 replay semantics) |
| `FixType` | `prompt.rs:248-262`: `#[serde(rename_all = "snake_case")] #[non_exhaustive] pub enum FixType { AuditFix, GapsOnly, FullExecute }` | `AuditFix` was Ship-review only | Add a quick-fix arm (D-10) and a stale re-verify arm (D-32); `AuditFix` loses its only producer with Ship (`pipeline_outcomes.rs:1026` `handle_ship_failure`) |
| Plan `--gaps` | `render_claude_style` Plan arm → `idempotent_stage_prompt`; `workflow_plan_prompt` (`prompt.rs:521-539`) says "if `PLAN.md` exists … do NOT run the workflow … report success" | **A gaps Plan pass would no-op**: plans already exist | Gaps Plan needs its own intent that skips the idempotency check |
| Validate verdict | `pipeline_launch.rs:3984-4002` (Advance arm) → `classify_validate_outcome` (`pipeline_outcomes.rs:230-273`) → `handle_validate_outcome` (`:523-764`); evidence root bound once at `:538-541` | Agent verdict only (999.193) | After `Passed`, call the GSD reader against the evidence root; route per D-08 |
| Validate → finish | `pipeline_outcomes.rs:556` (ambiguous approve), `:744` (gate approve), `:762` (auto pass) all call `transition(…, Stage::Ship)` | Ship | `finish_run` (D-04/D-05/D-29) |
| Gate text | `pipeline_outcomes.rs:697` `"Validation passed — approve to ship?"` | Ship wording | "approve finishing" wording + GSD status in context (D-04, D-08) |
| Loop-back fix selection | `select_loop_back_fix` (`pipeline_outcomes.rs:318`), `stamp_validate_dispatch_window` (`pipeline_launch.rs:112`); fields `last_verification_fingerprint` (`state.rs:199`), `verification_baseline_captured` (`:219`), `last_verification_mtime_nanos` (`:245`), `verification_run_nonce` (`:272`) | Authorship heuristic (999.192) | **Recommend removal**: status → fix is a total map (`gaps_found`→Plan gaps, `stale`/`missing`→execute re-verify, clean reject→quick), so the heuristic has no remaining decision. Removal touches 4 `pub` State fields (S) and ~60 references in CLI. Keep 999.192's test intent: "a verifier-written report from Code routes by GSD status" |
| Streak reset | `mode.rs:142-144` `transition_resets_consecutive_failures` (`!matches!((from, to), (Stage::Code, Stage::Validate))`); applied `pipeline_gate.rs:109-111` | Plan→Code zeroes the streak | D-25; **see Pitfall 3 — insufficient alone** |
| Progress predicate | `mode.rs:150-175` `consecutive_failures_made_progress`; count = `agent_result::phase_commit_count` (`agent_result.rs:2858-2898`, `rev-list --count develop..feature/phase-NN`, all commits) | Any new commit = progress → streak set to 1 (`pipeline_outcomes.rs:610-621`) | Plan `--gaps` always commits a PLAN.md; see Pitfall 3 |
| Caps | `mode.rs:18` `MAX_CONSECUTIVE_FAILURES: u32 = 3`, `:41` `MAX_PHASE_VALIDATE_FAILURES: u32 = 10`, `:68` `MAX_INFRA_FAILURES: u32 = 5` | — | unchanged (D-12) |
| Write-time 300 cap (999.206) | `pipeline_outcomes.rs:888-890` `truncate_reason` = `render_gate_context(reason, 300)`; callers `:553` (ambiguous), `:923` (`stage_failure_context`); also `pipeline_gate.rs:778` (`workflow_aborted` event), `pipeline_launch.rs:395,665,731`, `preflight.rs:814,846,1464,1472,1486` | Stored gate context already capped | Store full sanitized reason (bounded, e.g. 4 KB — `[ASSUMED]` size) in gate file + event; keep the 300 cap at **display** (`commands.rs:2056` `GATE_CONTEXT_DISPLAY_CHARS`, banner `:2061-2088`, list) **and add one at notify**: `run_notify_command` passes the context unbounded (`gates.rs:1546-1552` `.env("DEVFLOW_GATE_CONTEXT", context)`), so moving the cap off the write path would put multi-KB text in notifications again (13-06) |
| Way-on first | `stage_failure_context` (`pipeline_outcomes.rs:920-926`): `"[never-silent] stage {stage} failed: {lead}{reason} — human review needed (retry, loop-to-code, or abort)"` | Way-on sentence is at the END → cut off at 300 once reasons are long | Reorder: way-on sentence first, reason last |
| `gate show` | `commands.rs:4434-4471` (`render_gate_context(&gate.context, usize::MAX)`), test `gate_show_renders_full_untruncated_sanitized_context` (`commands.rs:10062`) | Already untruncated | Criterion-4 control is the >300-char reason surviving end to end |
| Rate-limit (no fallback) | `pipeline_launch.rs:4211-4236` (`Action::AutoResume`; `RateLimited` → `failover_target` → `fail_over_on_rate_limit` or `handle_rate_limited_outcome`; `_` arm also → `handle_rate_limited_outcome`, unreachable since only `RateLimited`/`Ambiguous` map to `AutoResume`, `outcome_policy.rs:41-47`); `pipeline_launch.rs:3328` (handoff refused → same); `pipeline_outcomes.rs:89-148` | Bumps `infra_failures`, writes cron, gate only if schedule empty | Interim (Wave 1): plain blocking gate, no infra bump (D-20). Final: D-15/D-16/D-27 gate |
| `resetsAt` | `agent_result.rs:1778-1789` (claude stream, epoch seconds via `json_scalar_to_string`), `:2786-2790` `rate_limited_result` → reason `"rate limited until {retry}"`; codex hint `detect_codex_rate_limit` | Free text in `reason` | Parse with the relocated `parse_retry_timestamp` (handles epoch seconds and RFC 3339, `ship.rs:357-367`); `AgentResult` has no reset field (`agent_result.rs:19-24`) |
| Gate wait | `run_gate_with_timeout` (`pipeline_gate.rs:521-733`); `GateWaitEnd { Answered, TimedOut, Stopped }` (`pipeline_launch.rs:1403-1410`); `auto_response` written via `Gates::answer` first-writer-wins (`pipeline_gate.rs:607-628`) | — | Reuse the `auto_response` write for the timer (see §Patterns) |
| Timeout / sweep | `config_parse.rs:19-22` `THREE_DAYS` default for `DEVFLOW_GATE_TIMEOUT_SECS`; `:97-100` sweep threshold; `gate_sweep_may_reap(_stage, waiter_live)` (`commands.rs:2684-2686`) | Sweep reaps by age | Sweep must skip a rate-limit gate before its reset (D-27.2): record a not-before time in the gate file (`GateFile`, `gates.rs:37-49`, also not `#[non_exhaustive]` — S) |
| Resume refusal | `refuse_launch_over_a_live_run` `commands.rs:4687` (called `:1010`); resume closes gates `pipeline_launch.rs:3512` `close_gates_for_resume` | — | Gate context names resume as "when the waiter is gone" (D-27.5) |
| `workflow_finished` | `pipeline_gate.rs:88-96` (`--until` halt, `{"reason":"stopped_at","stage":…}`) and `:380-385` (terminal, `Value::Null`) | No notify | D-29 payloads; notify via `gates::fire_gate_notify_with_fallback` (`gates.rs:1476-1488`) + `notify_fired` event like `pipeline_gate.rs:601-606` |
| OPS-01 `--version` | `main.rs:36-40` `#[command(name = "devflow", version, …)]`; `build.rs:60` `DEVFLOW_BUILD_COMMIT`, `:69` `DEVFLOW_BUILD_DIRTY` | `devflow 3.0.0` | Emit a combined version string from `build.rs` (e.g. `cargo:rustc-env=DEVFLOW_VERSION_LONG=…`) and use `#[command(version = env!(…))]`; commit may be empty (`build_provenance.rs:22` `build_commit_is_empty_or_a_full_hex_sha`) → print `unknown` |
| OPS-02 cleanup | `commands.rs:1300-1418`; message at `:1346` says "halted via --until" for **any** `stopped` | Wrong cause; `--force` leaves `state-NN.json` (999.213) | Print `stop_reason`; `--force` clears that run's state; skip merged-branch delete for a branch a kept worktree uses (`git.cleanup_merged` `:1421`) |
| OPS-02/04 status | `commands.rs:1862-1960`; no `stop_reason`, `fallback_agent` (`state.rs:482`), `fallback_consumed` (`:491`) reader | — | Add lines; "worktree gone → not active" check; drop cron hints `:1955` |
| OPS-02 start | `commands.rs:261` `fresh_state_carrying_phase_failures`, `:956` `keep_unloadable_state` | Silent replace | Say so |
| OPS-03 tracing | `main.rs:578-591` both branches `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))` | INFO on stderr by default | Default `off`/`warn` when `RUST_LOG` unset — **conflicts with `log_format_env.rs:167` `rust_log_unset_still_shows_info_level_logs_by_default`** (must be inverted, becoming the negative-direction test). Decide what the detached `__monitor` logs (its stderr is `/dev/null`; 999.217 suggests routing to the monitor log) |
| OPS-03 doctor | `commands.rs:5621-5651`: unset → `("missing", "not set — defaulting to info", hint)` | red ✗ | Report as info |
| OPS-03 provenance | `staleness.rs:497-503` `StalenessOutcome::Warn` prints for any non-self-dogfood project | Noise | Silent when `!self_dogfood` (`is_self_dogfood_workspace`, `staleness.rs:341`); control: still fires on DevFlow's own checkout |
| OPS-03 missing remote | `preflight.rs:380-386` (fetch failed) and `:572-575` (`undeterminable_currency_warning`) | Two lines | One line when there is no remote |

## GSD Integration (D-08, D-22, D-24, D-32, D-33)

**Today DevFlow never invokes gsd-tools in production.** The only call is a test in `phase_id.rs:419-480` that searches `PATH` for `gsd-tools` and prints a NOTICE and passes when absent (a fail-open precedent — do not copy it) `[VERIFIED: phase_id.rs]`.

**Probe (this session, gsd-core 1.20.1, scratch repo, cleaned up)** `[VERIFIED: live probe]`:

| Call | Case | stdout (JSON) | exit |
|---|---|---|---|
| `query verification status <phase_dir>` | no report | `"status": "missing"`, `"route": "execute-phase"`, `"next_command": "/gsd-execute-phase 01"` | 0 |
| same | phase dir absent | `"status": "phase_dir_not_found"`, `"route": ""`, `"message": "Usage error: …"` | **0** |
| same | report `passed`, covered file unchanged | `"status": "passed"`, `"route": ""` | 0 |
| same | `passed`, covered file changed (uncommitted is enough) | `"status": "stale"`, `"route": "execute-phase"`, `"moved_files": ["a.txt"]` | 0 |
| same | `gaps_found`, covered file changed | `"status": "gaps_found"`, `"route": "plan-phase"`, `"next_command": "/gsd-plan-phase 01 --gaps"` | 0 |
| same | `status: bogus` | stderr `Error: Verification report … has status "bogus", which is outside the closed set …`, no JSON | **1** |
| `query frontmatter get <report> --field covered_files` | present | `{"covered_files": ["a.txt"]}` | 0 |
| same, `--field nope` | absent | `{"error": "Field not found", "field": "nope"}` | **0** |
| `query verification fingerprint <phase_dir> a.txt` | ok | `{"covered_files": [...], "covered_digest": "v4:sha256:…"}` | 0 |

Status enum, verbatim (`~/.claude/gsd-core/bin/lib/verification.cjs:91-99`): `PASSED: 'passed'`, `GAPS_FOUND: 'gaps_found'`, `HUMAN_NEEDED: 'human_needed'`, `STALE: 'stale'`, `MISSING: 'missing'`, `UNPARSEABLE: 'unparseable'`, `PHASE_DIR_NOT_FOUND: 'phase_dir_not_found'`. Routes (`:268-316`): `passed`→`''`, `gaps_found`→`plan-phase` + `' --gaps'`, `human_needed`→`verify-work`, `stale`→`execute-phase`, `missing`→`execute-phase`, `unparseable`→`''`, `phase_dir_not_found`→`''`. A result may also carry `staleCheckIndeterminate: true` (GSD's own fail-open, `:1490-1494`) `[VERIFIED: verification.cjs]`.

**Parser contract (fail-closed, D-24):** treat as a reader failure → never-silent gate naming it: spawn error, exit ≠ 0, stdout not JSON, status not in the 7-value set, `phase_dir_not_found`, `unparseable`, `passed` **with** `staleCheckIndeterminate: true` `[ASSUMED: last one is a recommendation]`, frontmatter result carrying `"error"`, fingerprint non-zero exit (a missing covered file fails the whole command, `verification.cjs:1852-1854`). Route on `route`/`status`, never on `next_command` (it is runtime-projected: Codex sees `$gsd-…`, `verification.cjs:252-255`).

**D-22 digest compare caveat:** `verification fingerprint` always computes the current version (`FINGERPRINT_VERSION = 4`, `verification.cjs:623`; CLI uses it at `:1914,1923`). A report stored with a `v1:`–`v3:` digest will never match a recompute → compare only when the stored prefix is `v4:`; otherwise fire the D-11 gate with "cannot recompute this report's digest version" (gate is the safe direction). Also v3+ digests union the phase's own live plan/summary artifacts (`:1556-1566`), so the recompute must be run with the same phase dir.

**Locating the reader:** no `gsd`/`gsd-tools` on `PATH` is reliable here (`~/.local/share/mise/shims/gsd` exists but `mise which gsd` reports it inactive) `[VERIFIED: shell]`. Each runtime install ships `gsd-core/bin/gsd_run`, a POSIX `sh` launcher that `exec node "$dir/gsd-tools.cjs" "$@"` `[VERIFIED: ~/.claude/gsd-core/bin/gsd_run]`. **Recommend** resolving `$HOME/.claude/gsd-core/bin/gsd_run` (then `$HOME/.codex/gsd-core/bin/…`, `$HOME/.pi/agent/gsd-core/bin/…` as fallbacks — `[ASSUMED]` that those dirs carry `gsd_run`; `~/.codex/gsd-core/bin` and `~/.pi/agent/gsd-core/bin` do contain `gsd-tools.cjs`), invoked with `Command::new(path).args([...])` (argv, never `sh -c`), `current_dir(evidence_root)`, a timeout, and `env_remove` of `GIT_DIR`/`GIT_WORK_TREE`/… (hermeticity rule). Because the launcher is `sh`, e2e tests plant a stub `gsd_run` script in `common::HermeticHome` that prints recorded JSON — no `node` needed in CI. Record the GSD version once per run (`runtime-identity --raw` → `{"packageName":"@opengsd/gsd-core","version":"1.20.1"}`) in an event.

**Evidence root:** use the same binding as `handle_validate_outcome` (`pipeline_outcomes.rs:538-541`: `state.worktree_path.clone().unwrap_or_else(|| project_root.to_path_buf())`), and resolve the phase dir by the prefix scan `phase_verification_path` uses (`agent_result.rs:3817-3833`, `"{padded}-"` prefix under `.planning/phases`) — but return the **directory**, since `status` must also see "dir exists, no report" (`missing`).

**D-33 file list:** the changed files of the fix commits need a recorded base commit at fix launch (new state field) → `git diff --name-only <base>..HEAD` in the evidence root, minus `.planning/`, minus `covered_files`. Note `covered_files` includes `.planning/` paths (evidence report `final-01-VERIFICATION.md` lists `01-01-PLAN.md`, `01-01-SUMMARY.md`) `[VERIFIED: 52-gap-loop-evidence]`.

## Architecture Patterns

### Data flow after the phase

```
start ──► Plan ──► Code ──► Validate agent ──► verdict pass? ──no──► (Auto: loop / Supervise: gate)
                     ▲                              │yes
                     │                              ▼
                     │                     GSD reader: status(evidence_root/phase_dir)
                     │        ┌─────────────┬───────┴───────┬──────────────┬──────────────┐
                     │      passed       gaps_found      stale|missing   human_needed   reader failure
                     │        │              │               │              │              │
                     │   (Supervise gate)  stale-gaps?    Code: execute    gate         never-silent gate
                     │   approve→finish_run  │yes→gate (D-11)  re-verify prompt (D-32)
                     │   reject→quick fix    │no                │
                     │        │              ▼                  ▼
                     │        │      Plan --gaps → Code --gaps-only    post-check: still stale → gate
                     └────────┴──────────────┴──────────────────┘
finish_run: workflow_finished {"reason":"finished","shipped":false} → notify once → state per OPS-02 → worktree kept
Rate limit, no fallback: gate(context: reset time, resume cmd) ──timer at resetsAt+margin──► Gates::answer(approve, by "devflow-rate-limit-timer")
                                              (used-flag persisted first; first-writer-wins vs human answer; one claim)
```

### Pattern 1: the reset timer is just another answer writer
**What:** At `resetsAt + margin`, the waiting supervisor persists `rate_limit_auto_relaunch_used = true` (state is `#[non_exhaustive]`, additive), then writes an approve via `Gates::answer` exactly like the existing `auto_response` path (`pipeline_gate.rs:607-628`). `GateError::AlreadyResponded` means a human answered first — read that answer instead. The existing single `Gates::claim` then arbitrates, so a timer and an answer can never both launch (D-27.3).
**Wait bound:** wait timeout = `max(gate_timeout_secs(), resetsAt + margin − now)` (D-27.2); sweep skips the gate before its not-before time.
**Second denial:** the relaunch's own rate-limit outcome finds `used == true` → plain gate, no timer.

### Pattern 2: terminal finish is a small function, not a stage
`finish_run`: close the Validate gate, emit `workflow_finished` `{"reason":"finished","shipped":false}`, fire notify once + `notify_fired`, deregister from the root registry (as `finish_workflow` did, `pipeline_gate.rs:359`), and leave worktree/branch. Whether the run's state file is **kept (marked stopped with a finished reason)** or cleared is an open question (see Open Questions 2) — it decides what `status`/`cleanup` can say.

### Pattern 3: source-scan "no literal left" test (criterion 2)
Model on `doc_check::no_monitor_pid_field_json_key_or_help_mention_remains` (`doc_check.rs:597-671`): forbidden tokens built with `concat!` so the test file never contains them contiguously (`REMOVED_STATE_FIELD`, `doc_check.rs:506`); comments removed with `strip_rust_comments` (`:1233`) for Rust and `strip_html_comments` (`:673`) for Markdown; planted positive and negative lines per token class; then the real scan over `rust_file_paths()` + scoped docs + `docs/**` + snapshots, **excluding `CHANGELOG.md`**. Token list must be case-sensitive and word-bounded — bare `Merge`/`Define`/`Ship` appear legitimately in prose and git terms. Suggested tokens: `cron-instructions`, `hermes_cron`, `yes_ship`, `yes-ship`, `YES_SHIP`, `VersionBump`, `ChangelogAppend`, `BranchCleanup`, `Hook::Merge`, `DocsUpdate`, `Stage::Define`, `Stage::Ship`, quoted stage values `"ship"`/`"define"` in JSON-ish fixtures, `devflow ship`, `devflow parallel`, `devflow evidence`, `devflow reference`, `reap-strays`, `legacy-claude-launch`, `LEGACY_LAUNCH`, `gsd-ship`.

### Anti-Patterns to Avoid
- **Deleting `MonitorLaunch::Legacy`** because it says "Legacy": it is the capture shape for every non-stream agent.
- **Keying "run not launched yet" on a stage value** after Define is gone without a test per site (`commands.rs:600,889,6681`).
- **Treating GSD exit 0 as success** (`phase_dir_not_found`, `{"error":…}` both exit 0).
- **A trailing docs plan**: doc_check fails the plan that removes a verb if its docs still name it.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---|---|---|---|
| VERIFICATION.md status/staleness | frontmatter parse | `gsd query verification status` | D-08; GSD's staleness rules (digest versions, live artifacts) |
| Covered-digest recompute | own hasher | `gsd query verification fingerprint` | v4 semantics include phase artifacts |
| Timer vs human race | second lock/flag protocol | `Gates::answer` first-writer-wins + `Gates::claim` | Already proven under the gate test suite (`gates.rs` ~96 tests) |
| Reset-time parsing | new date code | relocated `ship.rs` parser (`:304-490`, with its unit tests) | Handles epoch + RFC 3339 + offsets |
| "No literal left" | shell `rg -c` | doc_check-style Rust test | CLAUDE.md repo trap |

## Runtime State Inventory

| Category | Items Found | Action Required |
|---|---|---|
| Stored data | `.devflow/state-NN.json` and gate files with `"stage": "define"`/`"ship"` stop parsing (`State.stage: Stage`, `state.rs:38`; `GateFile.stage`, `gates.rs:42`); serde ignores unknown keys (no `deny_unknown_fields` in `state.rs`/`gates.rs`), so removed fields like `yes_ship`/`legacy_claude_launch` are harmless. Main checkout `.devflow/` today: **no state files, empty `gates/`, no `cron-instructions*`, `lock-*` or `last-ship*`** `[VERIFIED: ls ~/Github/devflow/.devflow]`. `events.jsonl` keeps historical `workflow_shipped`/`hook_run` lines (append-only; `history` must still render unknown events). | Close-out: re-check the operator's `.devflow/` (and any throwaway project still in use) for 3.x states/gates/answers and remove them (D-28/D-30); no code migration |
| Live service config | Machine-global root registry `~/.cache/devflow/roots`: 1093 entries, 1069 distinct roots, only **1** still exists (`/var/home/denniyahh/Github/devflow`); newest entry 2026-10-19 `[VERIFIED: node scan]`. `gate list --all-roots`/`gate sweep` read gate files under each root. | None required; optional operator cleanup of stale `/tmp` entries (pre-hermetic test leak, not new) |
| OS-registered state | DevFlow never registered a cron job itself (999.186: nothing acted on the file). Whether the operator hand-created `hermes cron` jobs from old cron files is unknown `[ASSUMED: none]`. | Ask at close-out |
| Secrets / env vars | No `DEVFLOW_*` variable is exported in this session's environment `[VERIFIED: env]`; the operator's shell profiles were not read. A leftover export of `DEVFLOW_YES_SHIP`/`DEVFLOW_CLAUDE_LEGACY_LAUNCH`/`DEVFLOW_REVIEW_ANGLES` would be silently ignored after the cut. | None in code; mention in CHANGELOG |
| Build artifacts / installed | Installed `~/.cargo/bin/devflow` = 3.0.0 (needed for the criterion-1 control first, then reinstall after the phase per the dogfooding rule); `Cargo.lock` loses `semver`, `git-conventional` (if no other dependent) | Record the 3.0.0 dry run **before** reinstalling |

## Common Pitfalls

### Pitfall 1: e2e suites silently lose their capture-shape coverage
**What goes wrong:** removing `--legacy-claude-launch` breaks 10 e2e files that use it only to force the capture shape (`supervisor_e2e.rs:81-96` defines `Shape::Capture => &["--legacy-claude-launch"]`).
**How to avoid:** switch those runs to a capture-shape agent with a fake binary (`common::fake_bin_dir_for("codex", body)`); keep `supervisor_e2e.rs` `capture_supervisor_owns_a_fake_agent_and_records_devflow_result` green as the shape witness.
**Warning sign:** an e2e test that now only exercises the stream shape while its name says capture.

### Pitfall 2: Define removal changes "not yet launched" semantics
`run_never_prepared` (`commands.rs:889`), start's interactivity probe (`:600`), doctor `check_missing_branch` (`:6681`) and `State::new` (`state.rs:712`) all assume a fresh run sits at Define. Re-keying to Plan collides with a real Plan run in progress. Each needs a test that distinguishes "never launched" from "Plan running" (e.g. by `supervisor`/`pending_supervisor` being `None` and no `workflow_started`).

### Pitfall 3: D-25's control can pass while the real loop still never trips
The streak is reset to 1 whenever the phase branch gained commits since the last failure (`pipeline_outcomes.rs:610-621` via `mode::consecutive_failures_made_progress`). A gap round always commits (gap PLAN.md, SUMMARY, re-verified VERIFICATION.md). Fixing only `transition_resets_consecutive_failures` leaves `MAX_CONSECUTIVE_FAILURES` unreachable on real runs; a stub-agent test that commits nothing would pass anyway (proxy measurement). **Needs a decision** (Open Question 1).

### Pitfall 4: GSD reader "success" that is not
Exit 0 with `phase_dir_not_found`; exit 0 with `{"error":"Field not found"}`; `passed` with `staleCheckIndeterminate`. Also running the reader against `project_root` instead of the worktree reads `missing` and loops on execute-phase (D-24).

### Pitfall 5: the gaps Plan pass no-ops
Both Plan renderers short-circuit when a `PLAN.md` exists (`prompt.rs:521-539` and the claude idempotent prompt). And the commit gate counts **all** commits ahead of develop (`agent_result.rs:2950-2975`, a known defect recorded in project memory), so a no-op gaps Plan still passes Layer 2. Add a post-check that a `gap_closure: true` plan newer than the report exists, or the round wastes a Code launch.

### Pitfall 6: moving the 300-char cap re-opens 13-06
Notify passes the context unbounded (`gates.rs:1552`). Bound it at the notify call when the write-time cap goes.

### Pitfall 7: OPS-03 vs an existing test
`log_format_env.rs:167` asserts INFO shows with `RUST_LOG` unset. It must be inverted in the same plan, and the detached supervisor's log destination decided, or OPS-03 lands red.

### Pitfall 8: criterion-1 control ordering
The 3.0.0 dry run must be recorded **before** any `cargo install` of the new build (installed binary is 3.0.0 now). `start --dry-run` writes no state `[ASSUMED]` — confirm with a scratch project, not DevFlow's own checkout.

### Pitfall 9: semver-checks is not wired into any gate
`cargo semver-checks` is absent from `check.sh` and CI. With `version = "3.0.0"` still in `Cargo.toml`, `check-release` reports the breaking removals as violations; bump to 4.0.0 (both places, `Cargo.toml:9` and `:20`) in the same plan that runs it, or pass `--release-type major`.

### Pitfall 10: test timing for the rate-limit timer
The Phase 51 fixture's `resetsAt` is `1785645600` (2026-08-02, already past) `[VERIFIED: failover_e2e.rs:45]` — under the new rule it triggers an immediate auto relaunch. Tests need computed `resetsAt` values (past → relaunch once; future + short `DEVFLOW_GATE_TIMEOUT_SECS` → still waits; second denial → gate stays) and a small margin, or the suite becomes slow. The margin is Claude's discretion; make it a constant the test can rely on (not an env var — D-16 forbids new config keys).

### Pitfall 11: GSD messages carry absolute paths
`phase_dir_not_found`'s `message` embeds the absolute phase path (probe output). `events.jsonl` must never carry a path (WR-02 precedent, `staleness.rs:474-483` comment). Put the status and a path-free label in events; the full message may go to the gate context and stderr only.

## Code Examples

Reader result shape to parse (verbatim probe output, gsd-core 1.20.1):
```json
{
  "status": "stale",
  "next_action": "Verification is stale — covered source files changed after the verifier last ran: \"a.txt\". Re-run execute-phase for this phase: …",
  "next_command": "/gsd-execute-phase 01",
  "route": "execute-phase",
  "moved_files": ["a.txt"]
}
```

Suggested fail-closed classification (sketch; names are `[ASSUMED]`, status strings are the verbatim enum above):
```rust
pub enum GsdStatus { Passed, GapsFound, HumanNeeded, Stale, Missing }
pub enum ReaderFailure { NotFound(PathBuf), Spawn(String), Exit(i32, String), NotJson, UnknownStatus(String), PhaseDirNotFound, Unparseable, Indeterminate }

fn classify(json: &serde_json::Value) -> Result<GsdStatus, ReaderFailure> {
    if json.get("staleCheckIndeterminate") == Some(&serde_json::Value::Bool(true)) {
        return Err(ReaderFailure::Indeterminate);
    }
    match json["status"].as_str() {          // index, never `.get().unwrap_or` (absent ≠ benign)
        Some("passed") => Ok(GsdStatus::Passed),
        Some("gaps_found") => Ok(GsdStatus::GapsFound),
        Some("human_needed") => Ok(GsdStatus::HumanNeeded),
        Some("stale") => Ok(GsdStatus::Stale),
        Some("missing") => Ok(GsdStatus::Missing),
        Some("unparseable") => Err(ReaderFailure::Unparseable),
        Some("phase_dir_not_found") => Err(ReaderFailure::PhaseDirNotFound),
        Some(other) => Err(ReaderFailure::UnknownStatus(other.to_owned())),
        None => Err(ReaderFailure::NotJson),
    }
}
```

## State of the Art

| Old (3.0.0) | New (4.0.0, this phase) | Impact |
|---|---|---|
| Define → Plan → Code → Validate → Ship + 5 hooks | Plan → Code → Validate, `finish_run` | `Stage` loses 2 variants (S) |
| Validate verdict = agent only | agent AND `gsd query verification status` | closes 999.193 |
| Loop back to Code with `GapsOnly`/`FullExecute` by authorship heuristic | route by GSD status; gaps → Plan `--gaps` → Code `--gaps-only` | closes 999.192/205 |
| Rate limit → cron file nothing reads | gate + one timed relaunch | closes 999.185/186 |
| Abort = note contains "abort" | explicit `abort` field + `gate abort` | closes 999.189 |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | CI image `mcr.microsoft.com/devcontainers/rust:2.0.13-1-bookworm` has no `node` | Summary, GSD integration | If it has node, tests could call the real reader; the stub design still works |
| A2 | `$HOME/.codex/gsd-core/bin` and `$HOME/.pi/agent/gsd-core/bin` ship `gsd_run` (only `gsd-tools.cjs` was seen there) | GSD integration | Fallback resolution misses; primary `~/.claude/...` path verified |
| A3 | Treating `staleCheckIndeterminate: true` as a reader failure | GSD integration | Could gate a run GSD considers fine; safe direction |
| A4 | 4 KB bound for the stored full reason | GATE/OPS table | Cosmetic |
| A5 | Release tool as an unpublished workspace binary | Release port | Planner may prefer script; D-02 requires stating why |
| A6 | `devflow start --dry-run` writes no state | Pitfall 8 | Control run could leave a state file; run it in a scratch project |
| A7 | `cargo semver-checks check-release` fails while `Cargo.toml` still says 3.0.0 | Pitfall 9 | Gate ordering only |
| A8 | No operator-created `hermes cron` jobs exist from old cron files | Runtime State | Stale external jobs could call `devflow resume` |
| A9 | Function/type names in the Code Examples sketch | Code Examples | None — illustrative |

## Open Questions (RESOLVED)

1. **What counts as "no progress" for D-25's three-round gate?** (needs an operator answer — it changes an existing guard) — **RESOLVED: D-34** (operator, 2026-11-09): only commits that change at least one path outside `.planning/` count as progress; this changes the 999.66 guard, and a docs-only Code fix also counts as no progress. Planned in 52-27.
   - Known: any new commit resets the streak to 1 (`pipeline_outcomes.rs:610-621`); every gap round commits `.planning/` files; D-25's control as written can pass with stub agents while real runs never trip it.
   - Options: (a) count only commits outside `.planning/` as progress, for gap rounds; (b) same rule for every Validate failure (also fixes the documented 33-RESEARCH weakness); (c) a separate gap-round counter that ignores commits.
   - Recommendation: (a) — smallest semantic change that makes the control honest; the per-phase 10 ceiling stays the backstop.
2. **After `finish_run`, is the run's state kept (marked stopped, reason "finished") or cleared?** — **RESOLVED: D-35** (operator, 2026-11-09): the state is kept, recorded stopped with reason `finished`; planned in 52-08 and 52-25.
   - Known: today the terminal finish clears state (`pipeline_gate.rs:355`); `cleanup` with no matching state removes the worktree without `--force` and tries to delete its branch (`commands.rs:1300-1418`), which would delete the worktree D-04 says to leave for the operator. OPS-02 wants cleanup to say why it keeps a worktree.
   - Recommendation: keep the state with `stopped: true` and a "finished" `stop_reason`; `resume` on a finished run refuses; `cleanup --force` clears it. Planner can decide (CUT-02/OPS-02 detail), but it must be decided before plan 5a.
3. **Supervise mode with a `gaps_found` Validate: what does `gate approve` do?** (agy review lead) — **RESOLVED: D-36** (operator, 2026-11-09): approve runs the gap loop, reject with a note steers it, abort ends the run; finishing over gaps is not offered. Planned in 52-27.
   - Known: supervise gates every Validate (`mode.rs:250-257`); D-04 says approve finishes; criterion 4 says the gate never reads "passed" over gaps.
   - Recommendation: on a `gaps_found` gate, approve runs GSD's gaps route (approve "the proposed next step"), reject with note runs it with the note, abort ends; finishing over gaps is not offered. This is an operator decision.
4. **Doctor's 2.x stray-process report** — **RESOLVED** (Claude's discretion, plan 52-03): the report stays and its hint is rewritten to a manual `ps -o args= -p <pid>` check then `kill <pid>`. The question as researched: the report keeps detecting strays but its repair (`--reap-strays`) is cut. Claude's discretion: rewrite the hint to a manual `kill <pid>` after identity check, or drop the report. Recommend the rewrite (smaller).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|---|---|---|---|---|
| node | real GSD reader (production) | ✓ host | v26.13.0 | — |
| gsd-core | D-08/D-22 readers | ✓ `~/.claude/gsd-core` | 1.20.1 | — |
| `gsd_run` launcher | reader invocation | ✓ `~/.claude/gsd-core/bin/gsd_run` | — | stub in tests |
| node in CI image | — | ✗ `[ASSUMED]` | — | stubbed `gsd_run` (`sh`) in hermetic HOME |
| cargo-nextest | plan gates | ✓ | 0.9.151 | — |
| cargo-semver-checks | CLAUDE.md semver rule | ✓ host only | 0.51.0 | run by hand in the bump plan |
| installed devflow | criterion-1 control | ✓ `~/.cargo/bin/devflow` | 3.0.0 | — |

**Missing with no fallback:** none. **Missing with fallback:** node in CI → stub.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `#[test]` via cargo test / cargo-nextest 0.9.151; insta snapshots (`INSTA_UPDATE=no` in `check.sh`) |
| Config file | none for nextest; `scripts/check.sh` |
| Quick run command | `cargo nextest run -p devflow-core --lib` and `cargo nextest run -p devflow --bin devflow` |
| Full suite command | `scripts/check.sh all` (fmt + clippy `-D warnings` + `cargo test --workspace --no-fail-fast`); parity `scripts/check-in-container.sh` |
| Per-test gate shape | `cargo nextest run -p <pkg> {--lib|--bin devflow|--test <file>} --no-tests=fail -E 'test(=<path>)'` — one per command |

### Phase Requirements → Test Map
Proposed names (new unless marked existing). Each row lists the passing direction and the control that must fail.

| Req | Behavior / passing direction | Control (must fail or must still fire) | Test (type) |
|---|---|---|---|
| CUT-01 | `start --dry-run` prints exactly plan, code, validate stage lines | Exact-list assertion fails on an empty or 3.0.0 output; 3.0.0 dry run recorded once in phase notes | `phase7_cli::start_dry_run_lists_exactly_plan_code_validate` (e2e); existing `start_dry_run_annotates_until_stage` rewritten |
| CUT-01 | `--until define` / `--until ship` rejected by clap | `--until plan` accepted | `phase7_cli::until_rejects_removed_stages` (e2e) |
| CUT-01 | "never launched" logic keyed off Plan | a Plan run in progress is not reported never-launched | `commands::tests::run_never_prepared_*` (unit, rewritten) |
| CUT-02 | Passing Validate → `workflow_finished` == `{"reason":"finished","shipped":false}`, worktree + branch present, `notify_fired` once | `--until plan` halt emits `reason: stopped_at` (distinct payload) | `supervisor_e2e::a_run_that_passes_validate_finishes_and_keeps_its_worktree` (e2e) |
| CUT-02/04 | `ship`, `parallel`, `evidence`, `reference`, `list`, `test`, `release` unknown to clap; absent from `--help` | `status` still known | `phase7_cli::removed_subcommands_are_unknown` (e2e) + existing `help_snapshot::help_output_matches_committed_snapshot` |
| CUT-03 | No-fallback rate limit → gate with reset time + resume command; no `cron-instructions-NN.json` | same fixture asserts both absence of file and presence of gate (one fixture, both directions) | `failover_e2e::a_rate_limited_claude_run_without_a_fallback_waits_at_a_gate` (rewrite of `:839`) |
| CUT-03 | Past `resetsAt` → exactly one auto relaunch; second denial → gate stays open, no 2nd relaunch | count `agent stage` invocations == 2 | `failover_e2e::a_passed_reset_relaunches_once_and_a_second_denial_waits` (e2e) |
| CUT-03 | Future reset outlives a short `DEVFLOW_GATE_TIMEOUT_SECS`; no reset time → plain gate; restart/resume does not re-arm | used-flag persisted | `failover_e2e::a_future_reset_outlives_the_gate_timeout`, `…::a_denial_without_a_reset_time_is_a_plain_gate`, `pipeline_launch::tests::resume_does_not_rearm_the_auto_relaunch` |
| CUT-03 | Sweep leaves a rate-limit gate before its reset | an ordinary aged gate is still reaped | `commands::tests::gate_sweep_skips_a_rate_limit_gate_before_its_reset` (unit) |
| CUT-04 | No removed-surface literal under `crates/`, `docs/`, scoped docs (comments stripped) | planted literal per token class is flagged; planted comment is not | `doc_check::no_removed_surface_literal_remains` (unit) |
| CUT-04 | docs ↔ source parity | removing a doc line for a kept verb fails | existing `doc_check::doc_referenced_identifiers_exist_in_source`, `doc_check::source_devflow_env_vars_and_subcommands_are_documented` |
| GATE-01 | Note with "abort" keeps run alive; explicit field aborts; approve+abort refused; field-less record reads not-abort | the old substring rule, kept as a fixture, fails the first assertion | `gates::tests::a_note_containing_abort_does_not_abort`, `…::an_explicit_abort_field_aborts`, `…::an_answer_that_approves_and_aborts_is_refused`, `…::a_record_without_the_abort_field_is_not_an_abort` (unit) |
| GATE-01 | Note verbatim in relaunched prompt, per shape | pre-fix prompt path (no note) fails the verbatim assertion | `gate_wedge_e2e::a_rejection_note_reaches_the_stream_prompt_verbatim`, `…::…_capture_prompt_verbatim` (e2e; fake agent logs stdin/argv) + `prompt::tests::fix_prompt_carries_the_note_byte_identical` |
| GATE-02 | Status → route map: gaps → Plan `--gaps`; clean reject → `/gsd-quick` + note; stale/missing → execute re-verify prompt quoting GSD route (D-32) | each wrong route asserted absent | `pipeline_outcomes::tests::*_routes_to_*` (unit) |
| GATE-02 | Reader failures (absent launcher, exit≠0, non-JSON, unknown status, `phase_dir_not_found`, `{"error"}`) each fire a gate | none of them reads as pass | `gsd_reader::tests::*` (core unit, stub launcher) |
| GATE-02 | Reader runs on the worktree evidence root | `project_root` read gives `missing` | `pipeline_outcomes::tests::the_reader_reads_the_worktree_evidence_root` |
| GATE-02 | D-33: uncovered new file → re-verify; `.planning/`-only → no re-verify | both cases in one test file | `pipeline_outcomes::tests::an_uncovered_fix_counts_as_stale`, `…::a_planning_only_fix_does_not` |
| GATE-03 | `gaps_found` driven to `passed` in-run (stub gsd_run sequence gaps_found → passed) | gate forced to "passed" over `gaps_found` fails the never-lies assertion | `supervisor_e2e::a_gaps_found_validate_runs_plan_gaps_then_execute_gaps_only_then_passes` (e2e) |
| GATE-03 | Three gap rounds with no progress force the gate (D-25, per Open Question 1) | rounds with source commits do not | `pipeline_outcomes::tests::three_gap_rounds_without_progress_force_the_gate` |
| GATE-03 | Stale `gaps_found` (digest mismatch) → D-11 gate; unchanged digest → gap loop; non-v4 digest → gate | — | `pipeline_outcomes::tests::a_stale_gaps_found_report_fires_the_no_route_gate` |
| GATE-03 | D-32: Code pass leaving report `stale` → gate, not pass | report `passed` after Code → Validate | `pipeline_outcomes::tests::a_code_pass_that_leaves_the_report_stale_ends_at_a_gate` |
| GATE-04 | >300-char reason reaches `gate show` whole; banner/list/notify still ≤300 and start with the way-on sentence | existing `commands::tests::gate_show_renders_full_untruncated_sanitized_context` | `pipeline_outcomes::tests::a_long_failure_reason_is_stored_whole` + e2e via `gate show` |
| OPS-01 | `--version` = `devflow 4.0.0 (<commit>, clean|dirty)` | empty commit renders `unknown` | `build_provenance::version_prints_the_embedded_commit_and_dirty_flag` (e2e) |
| OPS-02 | cleanup names `stop_reason`; `--force` removes state; status never shows a gone worktree as active; start announces stale replace | a run whose worktree is present still reports active | `phase7_cli::cleanup_*`, `commands::tests::status_*` |
| OPS-03 | No tracing lines with `RUST_LOG` unset; doctor RUST_LOG = info; no provenance warning off-repo; one missing-remote line | `RUST_LOG=debug` shows tracing (`log_format_env::rust_log_debug_is_honored_under_json_log_format`, existing); provenance still fires on DevFlow's own checkout (`staleness::tests::enforce_build_staleness_blocks_self_dogfood_behind_worktree_head`, existing) | `log_format_env::rust_log_unset_prints_no_tracing_lines` (inverts `:167`); `staleness::tests::…ordinary_project…` (inverts `:2168`) |
| OPS-04 | status prints stop reason and fallback `armed`/`spent`/`none` | each of the three values | `commands::tests::status_prints_the_stop_reason_and_fallback_state` |

### Sampling Rate
- **Per task commit:** the task's named nextest gates + `doc_check::doc_referenced_identifiers_exist_in_source` + `doc_check::source_devflow_env_vars_and_subcommands_are_documented` when docs or verbs changed.
- **Per wave merge:** `scripts/check.sh all`.
- **Phase gate:** `scripts/check.sh all` + `scripts/check-in-container.sh` + `cargo semver-checks check-release -p devflow-core` after the 4.0.0 bump.

### Wave 0 Gaps
- [ ] Stub `gsd_run` helper in `crates/devflow-cli/tests/common/mod.rs` (scripted JSON sequence per call, logs argv) — needed by every GATE-02/03 e2e test.
- [ ] Capture-shape fake agent helper replacing `--legacy-claude-launch` usage (plan 2).
- [ ] Fake agents that log stdin and argv, for the per-shape note assertion.
- [ ] Recorded gsd-core 1.20.1 JSON fixtures (the probe outputs above) for `gsd_reader` unit tests.

## Security Domain

`security_enforcement` is absent from `.planning/config.json` → enabled.

| ASVS Category | Applies | Standard Control |
|---|---|---|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | yes (gate answers) | first-writer-wins answer + single claim; timer can only answer the rate-limit gate kind |
| V5 Input Validation | yes | note: strip control chars except `\n`/`\t`, delimited block; GSD JSON validated against the closed status set |
| V6 Cryptography | no (digest computed by GSD) | — |
| V12 Files/Resources | yes | phase dir resolved under the evidence root by prefix scan; launcher path fixed under `$HOME` |
| V14 Configuration | yes | no new env vars/config keys (D-16); subprocess via argv, env scrubbed of `GIT_*` |

| Threat | STRIDE | Mitigation |
|---|---|---|
| Hand-written answer with `approved: true` and `abort: true` | Tampering | refuse at write **and** treat as invalid at read (a file can be written without the CLI) |
| Timer auto-approve reused for a non-rate-limit gate | Elevation | dedicated writer with no boolean parameter, refuses other gate kinds (mirrors `reap_answer`'s T-23-41 design, `gates.rs:672-679`) |
| Forged `passed` from a planted `gsd_run` | Spoofing | `$HOME` is operator-controlled (accepted); record `runtime-identity` version in an event |
| Reader hangs | DoS | bounded timeout → reader-failure gate |
| Absolute paths in events | Info disclosure | never copy GSD `message` into `events.jsonl` (Pitfall 11) |
| Prompt injection via note | Tampering | note is operator-authored; delimited block, no shell interpolation (argv / stdin only) |

## Sources

### Primary (HIGH)
- Source files read this session in `/var/home/denniyahh/Github/devflow/.worktrees/phase-52` at `f3c8600`: `crates/devflow-core/src/{stage,mode,gates,prompt,state,agent_result,ship,hooks,doc_check,history,events,supervisor,lib}.rs`; `crates/devflow-cli/src/{main,commands,pipeline_gate,pipeline_outcomes,pipeline_launch,parallel,preflight,staleness,config_parse}.rs`; tests listed in the tables; `doc-check-allowlist.toml`; `Cargo.toml`; `scripts/cut-release.sh`; `scripts/check.sh`; `.devcontainer/devcontainer.json`.
- `~/.claude/gsd-core/bin/lib/verification.cjs` (gsd-core 1.20.1) L91-99, L240-316, L623, L1036-1060, L1470-1570, L1780-1870; `~/.claude/gsd-core/bin/gsd_run`.
- Live probe of `query verification status|fingerprint`, `query frontmatter get` in a scratch git repo (removed after use).
- `.planning/phases/52-subtract-and-stabilize/{52-CONTEXT,52-GAP-LOOP-VERIFICATION}.md`, `52-context-review/review-{codex,pi,agy}.md` (agy first 60 lines), `.planning/REQUIREMENTS.md`, `.planning/ROADMAP.md` (Phase 52 and 999.189/190/192/193/202/205/206/208/213/216/217), `.planning/user/V4-CUT-INVENTORY.md`.

### Secondary / Tertiary
- None. No web sources were needed; this is an in-repo phase.

## Metadata

**Confidence breakdown:**
- Cut graph: HIGH — every reference set came from `rg` over the tree plus reads of the defining files; counts are per-file `rg -c` sums (comment lines included, so they over-count real code references slightly).
- GATE/OPS anchors: HIGH — each line re-read; ranges marked approximate where they span a function.
- GSD integration: MEDIUM — one probe per case on 1.20.1; no agent runs; CI node absence assumed.
- Pitfalls: HIGH for 1, 2, 4-7, 9-11 (code read); MEDIUM for 3 (mechanism verified, real-run frequency not measured); 8 assumed.

**What this research does not establish:** that the deletion order compiles at each step (no build was run, per the task's read-only constraint); exact test counts per file that will need rewriting (only literal-match counts); runtime of the new e2e tests.

**Research date:** 2026-11-09
**Valid until:** 2026-12-09, or until gsd-core changes `verification.cjs` (#6078/#6096) — re-probe the reader shapes then.
