---
gsd_state_version: 1.0
milestone: v2.0.0
milestone_name: milestone (open — no fixed closing phase)
current_phase: 21
current_phase_name: operator-legibility-observability
status: planning
stopped_at: "Phase 21 CONTEXT complete (operator-recut, consolidated to develop); ready for /gsd-plan-phase 21"
last_updated: "2026-07-23T18:20:00.000Z"
last_activity: 2026-07-23
last_activity_desc: "Phase 21 scope recut to Operator Legibility & Observability (21a 999.3, 21b 999.14, 21c 999.2, 21d 999.29, opt 21e 999.5); dogfood retired, work consolidated to develop"
progress:
  total_phases: 12
  completed_phases: 9
  total_plans: 70
  completed_plans: 70
  percent: 75
---

# DevFlow — Project State

> Last updated: 2026-07-22

## Active Phase

**Phase 21 — Operator Legibility & Observability** (planning). CONTEXT gathered
and operator-recut 2026-07-23; scope is operator-decided (not
`/gsd-review-backlog`-promoted). Units: 21a discoverability (999.3), 21b doctor
staleness reconciliation (999.14), 21c sequentagent second-process tracking
(999.2), 21d dogfood staleness content-awareness (999.29, sequence first),
optional 21e changelog content (999.5). The v2.0.0 milestone stays open (no fixed
closing phase); numbering continues forward until a breaking change earns 2.0.

> **Workflow note:** Phase 21 began as a devflow dogfood run (headless Define in a
> `feature/phase-21` worktree), then moved to **manual GSD**: the worktree/branch
> were retired and all define artifacts consolidated onto `develop`. Next step is
> a manual `/gsd-plan-phase 21`.

## Current Position

Phase: 21 (operator-legibility-observability) — PLANNING
Plan: 0 of N (CONTEXT complete; not yet broken down)
Status: Ready for `/gsd-plan-phase 21`. Define complete (`21-CONTEXT.md`); discuss re-run intentionally skipped — CONTEXT is operator-complete.
Last activity: 2026-07-23 — scope recut + consolidated to develop; 999.28/999.29 filed and mirrored to Linear (DEN-53/DEN-54)

Progress: Phase 20 shipped v1.7.0 (100%); Phase 21 in planning.

*(Machine-readable fields for `gsd-tools state begin-phase` / `advance-plan` —
this project historically tracked position only in the narrative "Active
Phase" section above, which `advance-plan` cannot parse (backlog 12,
`deferred-items.md`). This section restores the standard fields so
`begin-phase` can seed real Plan/Status values once Phase 20 starts.)*

## Recently Shipped

- **Phase 20 — Release Correctness + Operator Control (Complete + Verified +
  Shipped as v1.7.0 — 5/5 plans, 2026-07-23).** Promoted from backlog
  2026-07-22 via `/gsd-review-backlog` as five units: 20a — 999.24
  `VersionBump` workspace member self-pins (High/S, DEN-49); 20b — 999.23
  `phase7_cli.rs` git-fixture reliability + `cleanup --force` liveness gate
  (High/M, DEN-48); 20c — 999.6 `devflow start --until <stage>` (High/M,
  DEN-31); 20d — 999.13 `devflow release --check` preflight (High/L,
  DEN-38); 20e — 999.7 manual `devflow ship --phase N [--force]` override
  (High/L, DEN-32).

  **Executed 2026-07-23** on `feat/phase-20-release-correctness-operator-control`
  via parallel git worktrees (Wave 1: 20-01+20-02 disjoint-file parallel;
  Waves 2-4 single-plan by DAG construction — 20-03→20-04→20-05 share
  `main.rs`). Post-merge gates green after every wave; final workspace suite
  480/0 pre-review-fixes.

  **Code-reviewed** (`20-REVIEW.md`): 2 blocker + 3 warning findings, all
  fixed inline before merge (11 new regression tests) — CR-01 (`version.rs`
  read/write asymmetry on a trailing TOML comment), CR-02 (a genuine
  cross-plan regression: `cleanup` never learned 20c's new `state.stopped`,
  so it could delete a `--until`-parked phase's worktree), WR-01 (dry-run
  `--until` visibility), WR-02 (bounded foreground `ship --phase` timeout),
  WR-03 (long-form `[dependencies.NAME]` publish-order parsing). INF-01
  (inline signing-key classification) deferred to backlog `999.27` / Linear
  DEN-52. Final: 491 tests / 0 failed, clippy + fmt clean.

  **Verified** (`20-VERIFICATION.md`): 37/37 must-haves confirmed against
  live source (re-run tests, not self-report). **UAT** (`20-UAT.md`): 2/2
  passed — the real ssh-agent signing-viability check was live-verified
  against the operator's actual setup across all 4 states (correct key / no
  agent / empty agent / unrelated key), and CI-on-branch sign-off for the
  two ex-flaky `phase7_cli.rs` fixtures was confirmed from PR #20's own CI
  logs (both named fixtures pass on both workflow runs). **Security**
  (`20-SECURITY.md`): 18 threats built from all 5 plans' `<threat_model>`
  blocks, 0 open, ASVS L1 short-circuit (register authored at plan time,
  all 11 named regression tests grep-confirmed present).

  **PR #20** (`feat/... → develop`) opened, CI green (8/8 checks), squash-
  merged to `develop` (`e78bc82`) 2026-07-23. **Ships as v1.7.0, not
  v2.0.0** — decided at ship time: nothing across the five units is
  breaking, and the v2.0.0 milestone stays open rather than closing here
  (see ROADMAP.md "Milestone stays open," 2026-07-23).

  **Fully released 2026-07-23.** `sync-main-to-develop.sh` run first (had
  not been run after v1.6.0 — `origin/main` had diverged from `develop`;
  caught by this phase's own new `devflow release --check` tool, then
  fixed the standard way). Version bump PR #21 (`chore/release-v1.7.0`,
  `Cargo.toml`/`Cargo.lock`/`CHANGELOG.md`) squash-merged to `develop`
  (`d1cf508`). Release PR #22 (`release: v1.7.0`, `develop → main`)
  squash-merged to `main` (`5c7259a`). Signed tag `v1.7.0` pushed
  (ED25519, verified). [GitHub Release](https://github.com/denniyahh/devflow/releases/tag/v1.7.0)
  published. `devflow-core` then `devflow` published to crates.io
  (`cargo search` confirms both at `1.7.0`). `develop` re-synced from
  `main` post-release. `devflow release --check` — the tool this very
  phase built — passed clean at every stage of the cut.

- **Phase 19 — Release Integrity + `main.rs` Decomposition (Complete + Verified
  2026-07-22 — 11/11 plans).** Targets **v1.6.0**. Promoted from backlog 2026-07-21 via
  `/gsd-review-backlog` as four units:

  - **19a** — 999.10 `.devflow/` artifact hygiene (Urgent/S). The only item
    whose blast radius reaches other people's repositories: `hooks.rs:184`'s
    `commit_all` sweeps unredacted agent stdout into a *user's* commit, and
    `main.rs:902` writes the operator's absolute home path and OS username
    into `events.jsonl`.

  - **19b** — 999.11 `commit_path` empty commits (High/S). `--allow-empty`
    means a release tag can sit on a commit containing nothing.

  - **19c–19f** — 999.8 split `main.rs` (High/L). Pure move, zero behavioral
    change. Now unblocked by Phase 18.

  - **19g** — 999.16 AI change acceptance contract (High/M). No Rust source;
    fully parallel track.

  Sequencing is load-bearing: 19a/19b land *before* the split so they are
  small diffs against the familiar file, not against seven new modules.

  **Principal risk — `ENV_MUTEX`** (18 `.lock()` sites / 63 refs in
  `main.rs`): a repeat root cause across 19i, GAP-2 and 999.4. If its
  serialization guarantees cannot survive distribution across module
  boundaries, that is a finding to surface, not to patch around. Verification
  must be CI-on-branch — local-green is explicitly insufficient.

  **Re-verified at promotion time:** all four source claims still hold at
  HEAD. `main.rs` is now **8,467 lines** (4,025 production + 4,442 test, 106
  tests) — **+35%** since 999.8 was written at 6,239, so its recorded cluster
  line ranges are stale and must be re-measured at plan time.

  **Discussed 2026-07-21** (`19-CONTEXT.md`, 21 decisions across 5 areas;
  alternatives preserved in `19-DISCUSSION-LOG.md`). Two decisions were made
  against evidence gathered during scouting rather than preference:

  - **`ENV_MUTEX` is not one mutex.** Three independent `static Mutex<()>`
    definitions exist (`main.rs:4034`, `gates.rs:348`, `config.rs:174`), sound
    today only because each guards a disjoint variable set — an accident,
    documented nowhere. `PATH` is mutated 36 times across 12 lock regions
    spanning 3+ target clusters, so per-module mutexes would silently break the
    exact serialization 19i raced on. **Decision: hoist one shared mutex into a
    test-support module.**

  - **The bottleneck is the pipeline state machine, not commands.** Mapping
    Phase 18's plans onto clusters: pipeline absorbed 3 of 7 plans (~1,040
    lines), commands only 2. A `commands/` subdirectory buys zero wave
    reduction. **Decision: flat siblings + split `pipeline.rs` at its natural
    seams**, which is what takes Phase 18's shape from 3 waves to 2.

  **Planned 2026-07-21 — 11 plans across 6 waves.** Research (HIGH confidence),
  pattern map, and VALIDATION.md all produced; `gsd-plan-checker` returned
  **VERIFICATION PASSED with 0 blockers and 0 warnings on the first
  iteration**. Wave 1 runs four plans in parallel (19a/19b/19g, disjoint file
  sets); waves 3–5 are one plan each by construction, since every split plan
  edits `main.rs`.

  **Two locked decisions were corrected against live source during planning:**

  - **D-14's named target does not exist.** `lock::ensure_devflow_dir` has zero
    matches in `crates/`; `.devflow/` is created from **7 independent
    `create_dir_all` sites**. RESEARCH.md's proposed `save_state` chokepoint was
    also verified **wrong** — `run_agent_blocking` (`main.rs:2417`), the
    `sequentagent`/`parallel` path, writes `.devflow/` on *"synthetic,
    never-persisted state"* and never calls `save_state`, so that chokepoint
    would leave the whole parallel path leaking. Corrected design: a new
    `workflow::ensure_devflow_dir()`, all 7 sites converted, plus a coverage
    test. Also reconciles the **duplicate `devflow_dir()`** (`workflow.rs:33`
    public, `agent_result.rs:872` private).

  - **D-20 was implemented but uncited**, so the blocking decision-coverage gate
    failed at 20/21. Now cited explicitly in 19-06 (the first split plan) with a
    halt-and-report branch if any 19a/19b work is still open when it starts.

  **Cross-AI reviewed 2026-07-21** (`19-REVIEWS.md`: Codex, OpenCode
  [`deepseek/deepseek-v4-pro`], Antigravity [via `agycli` — the working
  `antigravity-cli` binary, not the broken `agy`-\>GUI wrapper]; Cursor
  failed on an account usage limit, not a plan defect). All three completing
  reviewers independently re-derived and confirmed the phase's core source
  claims (the 7 `create_dir_all` sites, `ENV_MUTEX`'s single-mutex design,
  the `commit_path` dead-arm). One HIGH-severity claim (OpenCode: a 19g
  reference file "doesn't exist") was checked and refuted — the file exists;
  likely an OpenCode sandbox read-access artifact.

  **Incorporated via `/gsd-plan-phase 19 --reviews`** the same day — 9 of 11
  plans revised (19-04, 19-10 untouched), re-verified `VERIFICATION PASSED`
  with 0 blockers on the first iteration:

  - Baseline durability (Codex + OpenCode, MEDIUM): 19-06 now writes a
    **committed** `19-SPLIT-BASELINE-names.txt`; 19-07/08/09/11 diff against
    it instead of an ephemeral `/tmp` file.

  - `depends_on` metadata (Codex, downgraded HIGH\->MEDIUM after checking
    `execute-phase.md` — wave-number gating already enforces order
    regardless of `depends_on` content): 19-06 and 19-11's `depends_on`
    arrays corrected to match their stated prerequisites.

  - Five more LOW/MEDIUM fixes: CI-wording precision (19-11), monitor-process
    cleanup (19-01), a second dogfood anti-pattern shape (19-05), a
    deleted-`.gitignore` doc gap (19-01), locale-independent git string
    matching (19-03), and an `ensure_devflow_dir` relative-path edge case
    (19-01).

  **Completed and verified 2026-07-22.** The verifier passed 7/7 observable
  truths. The final proof matched all 438 test names and per-target counts,
  preserved the single CLI `ENV_MUTEX`, and passed three CI attempts on pushed
  SHA `aa95873`. The approved checkpoint records that symbol and name-set proofs
  ran locally against that exact SHA because the existing CI workflow does not
  contain those jobs.

- **Phase 18 (Complete + Verified + review-fixed + Released as v1.5.0 — 7/7 plans):** Dogfood
  Reliability Hardening — reprioritized 2026-07-20 from Hermes Support.
  `devflow doctor` reconciliation (18a), monitor liveness (18b),
  worktree-aware staleness enforcement (18c), Code↔Validate safety-gate
  reachability (18d), Layer 0/Validate verdict fix (18e), preflight-gate
  re-run wedge fix (18f), WR-03 test stabilization (18g). Replaces the
  fixed Phase 19 roadmap entry — see `## Backlog` in ROADMAP.md for the
  items not pulled into 18. Depends on Phase 17 (typed outcomes, build
  provenance).

  **Verified + reviewed 2026-07-21.** `gsd-verifier`: 7/7 must-haves,
  each traced to source plus an independently-executed passing test;
  both binding operator decisions (18e, 18f) confirmed exactly
  implemented (`18-VERIFICATION.md`, status passed). `gsd-code-reviewer`:
  0 critical / 4 warning (`18-REVIEW.md`). All findings dispositioned in
  a `18-fix` batch (6 commits `f635adf`..`4ff6b37`): WR-01 `doctor --json`
  now emits one JSON object `{environment, reconciliation}` (was two
  concatenated arrays = invalid single-doc JSON; proven fixed against the
  live binary); WR-04 `launch_stage_inner` clears `monitor_pid` before any
  fallible step so a failed relaunch no longer false-reports "Stuck";
  WR-03 the `unreachable!()` in `handle_validate_outcome` eliminated by
  construction (`ValidateResult` two-variant enum); WR-02 the
  `self_dogfood_stale_blocked` event now persists a path-free reason (third
  instance of that leak class — noted closed in `999.10`, the two original
  instances remain); and the new 18c worktree-staleness test hardened under
  `ENV_MUTEX` against the 19i PATH-race flake the verifier caught. Final
  gates: 426 tests / 0 failed, clippy `--workspace --all-targets` clean,
  fmt clean, all on `develop`. **Merged to main and released as v1.5.0**
  (2026-07-21, PR #12 squash-merged to `main`, signed tag `v1.5.0`,
  `devflow-core` + `devflow` published to crates.io). `develop` synced
  back from `main` post-release via `scripts/sync-main-to-develop.sh`.

  Planned 2026-07-20: research (HIGH confidence, all 7 defects re-verified
  as still reproducing at HEAD), VALIDATION.md (Nyquist), 7 plans, and a
  plan-checker pass that returned VERIFICATION PASSED with zero blockers
  and zero warnings on the first iteration. Waves are near-serial by
  necessity, not choice — six of seven fixes touch `main.rs` (6,239
  lines), and the same-wave zero-file-overlap rule forces one `main.rs`
  plan per wave.

  Executed 2026-07-20: **18-01 complete** (`8fdbd8a`, `3ce77a1`) —
  `devflow doctor` project-aware reconciliation (18a). `Severity`/
  `PhaseFacts`/`PhaseFinding`/`reconcile_phase` pure core plus
  `collect_phase_facts`/`render_reconciliation` wiring into `doctor()`'s
  text and `--json` output; 5 named checks (gate-pending-without-gate,
  orphan-gate, dead-agent, stage/event drift, missing feature branch), 10
  new tests, proven read-only by a twice-run fixture. See
  `18-01-SUMMARY.md`.

  Executed 2026-07-21: **18-02 complete** (`84afc3b`, `8dcc9ef`) — WR-03
  test stabilization (18g). `parallel_creates_two_worktrees_and_spawns_two_monitors`
  now asserts each stdout capture inside its own `wait_for` window instead
  of after a later, unrelated re-check. The plan's literal combined-assertion
  instruction was itself still racy — the mandated 25x loop reproduced a
  real failure at run 15/25 — so it was corrected to interleaved per-wait
  assertions, matching the plan's own must_haves.truths. 25/25 clean after
  the fix; `cargo test --workspace` 0 failed, `build_provenance` (WR-07,
  still open, out of scope) passed cleanly. See `18-02-SUMMARY.md`. Next:
  18-03 (wave 2).

  Executed 2026-07-21: **18-03 complete** (`9f33b75`, `05556a2`, `dbbff40`,
  `e60271d`) — monitor liveness (18b), "who watches the watcher."
  `State.monitor_pid: Option<u32>` persisted by `launch_stage` immediately
  after `monitor::spawn_monitor` returns (re-saved because `transition()`
  saves state before `launch_stage` runs, or the pid is lost); pure
  `liveness()` predicate (`Healthy`/`BetweenStages`/`Stuck`/`Unknown`,
  `None` matched first so an unrecorded monitor can never render `Stuck`)
  shared verbatim by `devflow status`'s new `monitor_pid`/`liveness` lines
  and `doctor`'s new `check_dead_monitor` finding, extending 18-01's
  `reconcile_phase` array right after `check_dead_agent`. 9 new tests;
  `cargo test --workspace` 405/405 (0 failed), clippy/fmt clean.
  Manually verified end-to-end against a synthetic dead-monitor fixture —
  `status` and `doctor` both correctly report `stuck — needs devflow
  resume` with a `devflow resume --phase N` repair, no filesystem paths
  or usernames leaked (WR-02 class). See `18-03-SUMMARY.md`. Next: 18-04
  (wave 3, 18d — make `MAX_CONSECUTIVE_FAILURES` reachable for the
  Code↔Validate loop).

  Executed 2026-07-21: **18-04 complete** (`37b74ac`, `3036927`) —
  Code↔Validate safety-gate reachability (18d). New pure `mode.rs`
  predicate `transition_resets_consecutive_failures(from, to)` — `false`
  only for `(Code, Validate)`, the mid-cycle hop that previously defeated
  the counter — consulted by `transition()` instead of an unconditional
  reset; `infra_failures`' unconditional reset is untouched, and the
  frozen regression test `transition_resets_infra_failures` passes
  byte-for-byte unchanged, proving 18d neither widened nor narrowed the
  infra counter's scope. `handle_validate_outcome`'s increment switched to
  `saturating_add`. RED-then-GREEN proven live:
  `consecutive_failures_reaches_ceiling_across_cycles` failed
  (`left: 0, right: 3`) against the unfixed `transition()`, passes after
  the fix. 6 new tests (2 in `mode.rs`, 4 in `main.rs` covering ceiling,
  saturation, idempotency, cross-phase independence); `cargo test
  --workspace` 411/411 (0 failed, up from 405), clippy/fmt clean. See
  `18-04-SUMMARY.md`. Next: 18-05 (wave 4, 18e — Layer 0/Validate verdict
  fix, causally entangled with 18d per 18-RESEARCH.md Pitfall 1).

  Executed 2026-07-21: **18-05 complete** (`1313ef9`, `e3eda07`,
  `1157d35`) — Layer 0/Validate verdict reconciliation (18e). New
  `reconcile_layer0_verdict` in `agent_result.rs` consults Layer 1's
  verdict when Layer 0 affirmatively succeeds at `Stage::Validate`
  instead of discarding it (copies ONLY `verdict`; `status`/
  `decided_by_layer`/etc. stay exactly as Layer 0 set them). New
  `ValidateOutcome` enum (`Passed`/`Failed`/`Ambiguous(String)`) and pure
  `classify_validate_outcome` in `main.rs` replace `handle_validate_outcome`'s
  old `passed: bool` — `Some(Verdict::Pass)` wins first (ordinary Validate
  unchanged), `(probe-pass, gaps)` and `(probe-pass, no-verdict)` classify
  `Ambiguous` and force an immediate `[never-silent]` gate that never
  touches `consecutive_failures` and never consults `Mode::should_gate`,
  per the binding 2026-07-20 operator decision (D-18e). Combined
  integration test `external_verify_cycles_reach_ceiling_without_unbounded_loop`
  proves 18d and 18e hold TOGETHER (18-RESEARCH.md Pitfall 1): an
  ambiguous outcome gates on cycle one without touching the counter, and
  a genuine repeated failure still reaches the now-reachable ceiling. 6
  new tests (2 in `agent_result.rs`, 4 in `main.rs`); `cargo test
  --workspace` 417/417 (0 failed, up from 411), clippy/fmt clean. See
  `18-05-SUMMARY.md`. Next: 18-06 (wave 5, 18c — worktree-aware staleness
  enforcement).

  Executed 2026-07-21: **18-06 complete** (`a80079f`, `10730ea`) —
  worktree-aware build staleness enforcement (18c), closing Round 4 CR-01.
  `enforce_build_staleness` now derives
  `execution_root = state.worktree_path.as_deref().unwrap_or(project_root)`
  (the same idiom `evaluate_layer0` already uses in `agent_result.rs`) and
  threads it through `embedded_commit_is_stale`/
  `tree_has_modified_build_inputs`/`combined_staleness` (parameter rename +
  call-site change only; ancestry exit-code contract untouched).
  `is_self_dogfood_workspace` and `events::emit` stay `project_root`-scoped
  (Assumption A3, documented in source) since they answer workspace-identity
  and bookkeeping-location questions, not staleness. Block message now names
  `execution_root` and states whether a worktree was in play. New
  `worktree_staleness_fixture` builds a real `git worktree add` fixture
  (sibling, not nested, directories — a nested worktree path would contain
  `project_root`'s path as a string prefix, making "message contains
  worktree path" and "message does not contain project_root path" mutually
  exclusive assertions). RED-then-GREEN proven live: manually reverted
  `execution_root` to `project_root` and confirmed the fix's own regression
  test fails, then restored and confirmed it passes. 3 new tests; `cargo
  test --workspace` 420/420 (0 failed, up from 417), clippy/fmt clean. See
  `18-06-SUMMARY.md`. Next: 18-07 (wave 6, 18f — preflight-gate re-run
  wedge fix).

  Executed 2026-07-21: **18-07 complete** (`a397d46`, `950a358`,
  `1ca79dd`) — preflight-gate re-run wedge fix (18f), the final plan of
  Phase 18. `launch_stage` split into itself (resolution + the
  `run_preflight` guard) and a new `launch_stage_inner` (everything after
  the guard); `run_preflight`'s `GateAction::Advance` arm now calls
  `launch_stage_inner` directly — skipping the just-adjudicated check
  entirely, per the binding 2026-07-20 operator decision (D-18f) — while
  `GateAction::LoopBack` still calls the full `launch_stage` (deliberately
  re-checking, since the operator may have fixed the condition). Either
  arm's recursion is bounded by a new persisted `State.preflight_retries: u32`
  against `mode::MAX_PREFLIGHT_RETRIES = 3`, checked BEFORE any new gate is
  written; reaching the ceiling emits `preflight_retry_ceiling_reached` and
  aborts instead of polling a second 7-day gate timeout. The counter resets
  to 0, persisted, on both a passing preflight and a human Advance. RED-
  then-GREEN proven live: manually reverted the Advance arm back to calling
  `launch_stage` and reproduced the documented wedge exactly (two gates
  written, then a bounded `"gate for stage define timed out awaiting a
  response"` error), then restored the fix and confirmed green. Deviated
  from the plan's literal Task 3 test setup (`Stage::Plan` +
  `AlwaysFailAdapter`) after confirming empirically it cannot reproduce a
  failure that survives a relaunch — `launch_stage`'s recursion always
  re-resolves the REAL production adapter via `agents::adapter_for`,
  discarding whatever adapter was passed into the outer `run_preflight`
  call — and used `preflight_interactivity_check` (a pure function of
  state, so it fails identically on retry) instead, the check CONTEXT.md
  actually attributes the wedge to. 4 new tests (1 in `state.rs`, 3 in
  `main.rs`); `cargo test --workspace` 424/424 (0 failed, up from 420),
  clippy/fmt clean. See `18-07-SUMMARY.md`. **Phase 18 (7/7 plans, 18a–18g)
  complete.**

## Backlog

**20 unsequenced items** remain in `.planning/phases/999.N-*/` and the
`## Backlog` section of ROADMAP.md. The first 16 were
reviewed/prioritized/sized 2026-07-21 (mirrored in Linear as
`DEN-26`..`DEN-45`); 999.21 and 999.22 were filed 2026-07-22 from Phase 19's
two retained non-blocking findings and mirrored as `DEN-46`/`DEN-47`:
acceptance-contract review wiring (999.21, High, DEN-46 — 19-05's dogfood found the
contract's wording works but its wiring doesn't: an isolated reviewer reaches
the right verdicts without citing the contract or grading at its blocking
severity) and refactor equivalence guard in CI (999.22, Medium, DEN-47 — the
symbol/name-set equivalence proof that validated the `main.rs` split runs only
locally; Phase 19 shipped with an explicit accepted override for exactly this).
999.23 (High, DEN-48) was filed the same day from the v1.6.0 release PR: a
flaky worktree-cleanup test in the release gate, proven a flake rather than a
regression. 999.24 (High, DEN-49) followed from the v1.6.0 release itself:
`VersionBump` bumps `[workspace.package] version` but not the
`[workspace.dependencies]` self-pin, so the pin has silently shipped stale two
releases running — invisible until `cargo publish` rejects it as a duplicate.
The 2026-07-21 sixteen: Hermes Support (999.1, Low),
phase-process tracking model (999.2, Medium — half-addressed by 18b's
`monitor_pid`), CLI operator discoverability (999.3, Low), version-tag
contention on concurrent ship (999.4, Medium), changelog placeholder content
(999.5, Low), plan-only pipeline mode (999.6, High), manual ship override
(999.7, High), dependency update review (999.9, Medium), Layer 0 veto test
coverage (999.12, Medium), release-cut automation (999.13, High), doctor
reconciliation for planning-doc staleness (999.14, Medium), shell-entrypoint
hermetic tests (999.15, High), mutation testing (999.17, Medium),
property/fuzz testing for parsers (999.18, Medium), fast/slow CI lanes
(999.19, Medium), differential coverage enforcement (999.20, Medium).
Promote with `/gsd-review-backlog`.

**Promoted into Phase 19 on 2026-07-21** (removed from the backlog):
999.10 `.devflow/` artifact hygiene (Urgent, DEN-35), 999.11 `commit_path`
empty commits (High, DEN-36), 999.8 split `main.rs` (High, DEN-33), 999.16
AI change acceptance contract (High, DEN-41). Their accumulated context was
consolidated into
`.planning/phases/19-release-integrity-main-rs-decomposition/CONTEXT.md`
as units 19a/19b/19c–19f/19g. Linear synced 2026-07-21: all four moved to the
`Phase 19: Release Integrity + main.rs Decomposition` project milestone,
retitled to their unit IDs, and set to Todo.

**Earmarked for Phase 20 (v2.0.0):** 999.6, 999.7, 999.13, likely 999.3 —
all four land in `main.rs`, which is why Phase 19's split precedes them.

Note: that same QA pass independently found and fixed an *unrelated* defect
in `verify.rs` (external-verification approval/frontmatter parsing accepted
empty commands, which `sh -c ""` silently passes) — not part of any backlog
item, already fixed and committed (`b1dcec7`), not a promotion candidate.

## Completed

| Phase | Description | Version | Date |
|---|---|---|---|
| 0 | Codebase map + Assessment | — | 2026-06-17 |
| 1 | CI Foundation + Test Coverage | — | 2026-06-17 |
| 2 | Version Bumper Expansion | — | 2026-06-17 |
| 3 | Verify & Docs Execution | — | 2026-06-17 |
| 4 | Hermes Skill | — | 2026-06-17 |
| 5 | Agent Trait Refactor | — | 2026-06-17 |
| 6 | Agent Completion + Ship Readiness | v0.5.1 | 2026-06-17 |
| 7 | Git Worktrees + PR Integration | v1.0.0 | 2026-06-18 |
| 8 | Docs + OSS Onboarding | v1.0.1 | 2026-06-18 |
| 9 | OSS Polish | v1.2.0 | 2026-06-18 |
| 10 | Logging + Planning Step | — | 2026-06-19 |
| 11 | GSD-Native Architecture + Remediation | v1.2.0 | 2026-06-20 |
| 12 | Bootstrap + Housekeeping | — | 2026-07-10 |
| 13 | MVP Core Loop | — | 2026-07-15 |
| 14 | Parallel Safety + Observability | — | 2026-07-16 |
| 15 | Dogfood Enablement + OSS Readiness | — | 2026-07-17 |
| 16 | Pipeline Reliability Hardening | — | 2026-07-17 |
| 17 | Pipeline Dogfood Follow-Up | — | 2026-07-19 |
| 18 | Dogfood Reliability Hardening | v1.5.0 | 2026-07-21 |
| 19 | Release Integrity + `main.rs` Decomposition | v1.6.0 | 2026-07-22 |
| 20 | Release Correctness + Operator Control | v1.7.0 | 2026-07-23 |

*Phases 8 and 10 shipped without a SUMMARY.md at the time; both were retroactively documented 2026-07-08 (see `8-SUMMARY.md`, `10-SUMMARY.md`) after reconstruction from git history. Phase 11 was reviewed and found already adequately closed out via `11-VALIDATION.md`/`11r-VALIDATION.md` (Nyquist-compliant, sign-off dated 2026-06-20) — no retroactive SUMMARY.md was needed.*

## Blockers

None currently open for Phase 17.

- **RESOLVED 2026-07-19 (17-09, `cb9359f`):**
  `concurrent_ship_advances_finish_both_phases_independently` no longer hangs.
  Mechanism (confirmed directly via temporary debug instrumentation, not just
  inferred from timing): the test ships phases 31 and 32 concurrently, and on
  a genuinely intermittent race (~33-40% of isolated runs, measured across
  three independent audits plus this fix's own 25-run verification), both
  `VersionBump` hooks compute the identical next version and race to create
  the same tag (`cannot lock ref 'refs/tags/...'`); the loser's ship failure
  reopens its Ship gate, and since the test only ever pre-wrote **one**
  response per phase, the reopened gate previously polled forever with no
  timeout. Fix: `DEVFLOW_GATE_TIMEOUT_SECS` is bounded to 2 seconds for this
  test's poll only (under the file's `ENV_MUTEX` guard, restored
  immediately after) — the 7-day production default is untouched. The test
  now asserts either legitimate outcome deterministically (no collision:
  both phases finish; collision: the loser's bounded timeout + intact,
  still-gated state). 25 consecutive isolated runs under a 120s external
  timeout: 0 hangs, 9 of which hit the race and resolved via the bounded
  path. **The underlying product-level version-tag contention (why the
  checkout lock occasionally doesn't fully serialize the two threads'
  terminal hooks) remains open and out of scope** — belongs to future
  ship/version-bump concurrency work, not Phase 17 or 18. See
  `17-VALIDATION.md` GAP-2 and `17-09-SUMMARY.md`.

## Decisions

| Date | Decision |
|---|---|
| 2026-07-23 | **Phase 20 executed on `feat/phase-20-release-correctness-operator-control` via parallel worktrees, not the #683 sequential degrade.** HEAD was 11 planning-only commits ahead of `origin/develop`, tripping the auto-degrade; operator chose to set `worktree.baseRef:"head"` and branch properly rather than commit straight onto `develop` (per the standing git-flow preference). Wave 1 (20-01+20-02, disjoint files) ran genuinely parallel; waves 2-4 were single-plan by DAG construction (20-03→20-04→20-05 share `main.rs`). Code review found 2 blockers (CR-01 version-read/comment asymmetry, CR-02 a genuine cross-plan regression where `cleanup` never learned 20c's `state.stopped`) + 3 warnings — operator chose to fix all 5 inline (11 new regression tests) rather than ship-then-backlog, since the branch wasn't merged yet. PR #20 opened `feat/... → develop`, CI green (8/8 checks, both named ex-flaky fixtures pass). Phase marked complete via `/gsd-verify-work 20` after live-verifying the ssh-signing UAT item against the real operator ssh-agent (4 states) and confirming CI-on-branch sign-off from PR #20's logs. `/gsd-secure-phase 20` ran State B (18 threats built from all 5 plans' `<threat_model>` blocks, 0 open, L1 short-circuit) since `workflow.security_enforcement=true` gated completion. **Milestone-boundary bug caught and corrected:** `phase.complete`'s next-phase detection picked up backlog heading `999.1` (Hermes Support) as if it were the next sequential phase — STATE.md's `current_phase`/`status` were corrected back to reflect that the v2.0.0 milestone (Phase 11-20) is 100% complete (`progress.percent: 100`, 9/9 phases) and awaiting an explicit `/gsd-complete-milestone` decision, not auto-continuation into a backlog item. PROJECT.md was also 3 phases stale (still described Phase 18 as "Hermes Support," which was rescoped away 2026-07-20) — evolved through Phases 18/19/20 in the same pass. INF-01 (signing-key inline-classification, Info-severity) deferred to backlog `999.27`/Linear DEN-52 rather than blocking the fix pass. |
| 2026-07-20 | **18-01: `cargo test -p devflow --lib` does not work on this crate — corrected in verification, not source.** 18-01-PLAN.md's own `<verify>`/`<acceptance_criteria>` blocks (and 18-RESEARCH.md's Validation Architecture table) specify `cargo test -p devflow --lib <name>`, but `devflow` (the `devflow-cli` package) is binary-only (no `[lib]` target), so `--lib` hard-errors (`no library targets found`, exit 101) rather than filtering tests. Used the working equivalent, `cargo test -p devflow <name>` (no `--lib`), for all verification in this plan and going forward. Flag this in future 18-0N plans' verify blocks so the same false-error isn't hit again. |
| 2026-07-20 | **18-01: two-task pure-core/wiring split requires staged `#[allow(dead_code)]` on a binary-only crate.** `crates/devflow-cli` has no `[lib]` target, so `cargo clippy --workspace --all-targets -- -D warnings` compiles the plain `bin` target *without* `#[cfg(test)]` — unit-test-only usage of a not-yet-wired item does not satisfy that build's dead-code check. Task 1 (pure `reconcile_phase` core) added `#[allow(dead_code)]` to its new items with a comment naming the exact commit that removes them; Task 2 removed every one once `doctor()` became the real caller. Verified clean independently after each commit (not just at the end). Pattern to reuse for any future plan that splits a pure-core commit from its wiring commit in this crate. |
| 2026-07-20 | **17-REVIEW.md WR backlog triaged to completion; four fixed, five backlogged, one annotated.** The 2026-07-20 Phase 18 restructure flagged WR-01/02/03/04/07/08/09/10/11 as never triaged into the roadmap. All were re-verified against HEAD rather than trusted from the review text (the WR-06 lesson). **Fixed immediately in `234f080`** as one quality-gate-integrity bundle: WR-10 (`devflow test` ran the narrow `cargo clippy -- -D warnings`, which does not compile test targets — a live false-green generator directly in Phase 18's path, since that phase adds substantial `#[cfg(test)]` code), WR-08 (no regression guard on clippy scope in either workflow; added guards over both workflow files plus `devflow test`, each RED-proven by reverting to the narrow form and confirming the intended diagnostic), WR-07 (no job timeouts — sharper after `f25c670` enabled all-branch CI, since a hung `build_provenance` would burn GitHub's 6-hour default), WR-09 (`CONTRIBUTING.md` still advertised the narrow clippy form). **Backlogged:** WR-01+WR-02 → `999.10` (grouped — WR-02 puts the developer's home path and OS username in `events.jsonl`, WR-01 commits it into the *user's* repo; highest severity of the batch since blast radius extends to other people's repositories, and Phase 18 fixes neither, citing WR-02 only as a prevention constraint), WR-03 → `999.11` (`--allow-empty` commits rather than skips, so a terminal-batch retry can tag a release on an empty commit), WR-04 → `999.12` (coverage debt on a deliberate trade). **Annotated in place:** WR-05 — `17-VERIFICATION.md`'s "at current HEAD" claim is scoped to `f5c399a` and does not cover 17-13's three commits; corrected with a scope note rather than re-running verification on a closed, shipped, merged phase, since 17-13's substance is independently confirmed by RED-proven regression tests and the Phase 18 research pass. **Already closed before triage:** WR-06 (by the roadmap restructure), WR-11 (is Phase 18's 18d). WR-04 was deliberately NOT folded into plan 18-05 despite touching the same file — 18-05 had passed the plan-checker clean, and growing a verified plan with adjacent debt is the scope-creep pattern that made prior phases balloon. |
| 2026-07-20 | **Phase 18 reprioritized to Dogfood Reliability Hardening; fixed Phase 19 eliminated in favor of a backlog:** operator call — dogfooding has repeatedly surfaced legitimate functional bugs that tax every subsequent run, so pipeline-self-correctness work (18a–18g, was 18d/18e + 19a/19d/19g/19k/19l) takes Phase 18's slot ahead of Hermes. Auditing the move surfaced two stale-documentation bugs of its own: 19e and 19f were already closed by 17-13 (`12b5b98`, `e421ebd` — RED-proven regression tests exist) but ROADMAP.md still described them as open; `17-REVIEW.md` WR-06 had already flagged this. Not carried forward. 19i was already resolved (`96411eb`/`40dade3`) before this restructure. Rather than open a new fixed Phase 19, the remaining real-but-lower-priority items (Hermes, 19b, 19c, 19h, 19j) moved to a GSD-native 999.x backlog (`## Backlog` in ROADMAP.md, `/gsd-review-backlog` to promote) — every prior phase renumbering in this project's history exists because "the next phase" kept absorbing newly-discovered work; the backlog gives that work a home that isn't a phase number. Dir renames: `18-hermes-support` → `999.1-hermes-support`; new `18-dogfood-reliability-hardening`, `999.2-phase-process-tracking-model`, `999.3-cli-operator-discoverability`, `999.4-version-tag-contention-concurrent-ship`, `999.5-changelog-placeholder-content`. `17-REVIEW.md`'s WR-07 (build_provenance test flake, no CI job timeout) and WR-01/02/03/04/08/09/10/11 were noticed during this audit but not triaged here — flagged for a follow-up review pass, not assumed resolved or added to the backlog sight-unseen. |
| 2026-07-18 | **Phase 17 scoped to four units; P5/P6 deferred to Phase 18:** source verification against final HEAD resolved decision-gate Q2 — `Unknown` auto-advance is not an edge case but an explicit design choice (`main.rs:854` classifies only `Failed \| RateLimited` as failure; `main.rs:871`'s comment states "Success (or Unknown — advance…)"). It is also broader than the retrospective recorded: `evaluate_layer3` (`agent_result.rs:610-620`) returns `Unknown` for the zero-commit "agent process gone, no commits" case too, so a vanished agent that did nothing advances Code→Validate. Two retrospective assumptions corrected: `devflow doctor` already exists but is project-blind (`_project_root` unused), and `RateLimited` is already typed — the missing outcomes are `resource_killed` (exit 137, absent workspace-wide) and `agent_unavailable`. Provenance has no foundation at all (no `build.rs`, no `vergen`; `workflow_started` carries only agent/mode/worktree). Phase 17 keeps 17a `Unknown` non-advance, 17b typed outcomes + retry policy, 17c preflight gate, 17d build provenance. Q4 answered: focused Phase 17 repair, **not** a Phase 16 remediation — only 17d traces to the proven Phase 16 defect; the rest is capability Phase 16 never claimed. Deferred to Phase 18 as 18d/18e: doctor reconciliation (forensic tooling, depends on 17b+17d) and the WR-03 test fix (test-only debt). Q3 (universal vs. adapter-specific preflight checks) remains open for discuss-phase. |
| 2026-07-17 | **New Phase 16 (Pipeline Reliability Hardening) inserted, Hermes Support renumbered 16→17:** dogfooding Phase 15 through DevFlow itself surfaced real pipeline gaps — two Code-stage false positives on the crates.io publish plan (Layer-2 commit-count heuristic once, an incorrect agent self-report once) and four consecutive Ship-review failures on distinct legitimate findings (leaked telemetry, incomplete gitignore fix, CI job that couldn't fail loud, a doc/behavior mismatch) that a single-pass standard-depth reviewer caught one at a time instead of together. Dir renamed `16-hermes-support` → `17-hermes-support`; new `16-pipeline-reliability-hardening` (neither had plans yet). |
| 2026-07-18 | **New Phase 17 (Pipeline Dogfood Follow-Up) inserted, Hermes Support renumbered 17→18:** Phase 16 execution evidence may show a failed Merge followed by VersionBump, BranchCleanup, and `workflow_finished`, contradicting the phase's fail-closed terminal contract. The Phase 17 spike captures this required final-HEAD reproduction plus outcome classification, preflight readiness, state/event reconciliation, and WR-03 test stabilization. Dir renamed `17-hermes-support` → `18-hermes-support`; Hermes remains scoped and blocked on the decision gate. |
| 2026-07-16 | **Phase 15 rescoped dogfood-first:** operator priority is a fully functional MVP for dogfooding. The MVP engine is done (13 + 14); the remaining friction is operational: gate responses required hand-writing `.devflow/gates/NN-stage.response.json`, and no accurate operator reference exists. Phase 15 now leads with 15a Dogfood Enablement (`devflow gate` list/approve/reject, OPERATIONS.md, plus pulled-forward accuracy items: `.devflow.yaml` decoy removal, IN-01 lib.rs rustdoc, `--help` snapshot test); 15b OSS packaging follows and is to be executed through DevFlow itself as the first post-MVP dogfood run. Antigravity adapter (old 15c) deferred to unscheduled backlog — serves neither priority. Phase 14 was merged to develop (431c743) before this rescope. |
| 2026-07-16 | **Phase 14 post-ship code review + fixes:** independent high-effort review (8 finder angles, 1-vote verification) found 10 issues — 2 critical (recover --clean wiped live sibling phases; checkout-lock timeout ran hooks unserialized), 7 warning, 1 info — all documented in `14-REVIEW.md` and resolved in `14-REVIEW-FIX.md` (7 fixed, 2 mitigated, 1 accepted-by-design). Notable policy calls: `recover --clean` now sweeps stale phases only with `--phase N` as the explicit escape hatch; a checkout-lock timeout skips the hook batch rather than ever mutating the checkout unserialized (`DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` tunable); agent binaries are preflighted before any monitor spawns. |
| 2026-07-16 | **Phase 14 complete — CR-03 closed:** per-phase `state-{NN}.json` + `advance --phase N` threaded from the monitor at spawn time (no shared singleton, pre-lock read deleted), two-level locking (per-phase advance lock + seconds-scale `lock-project` around all primary-checkout git mutation), per-phase `cron-instructions-{NN}.json`, sequentagent behind a no-advance monitor holding its phase lock (sync `launch_agent`/`capture_agent_output` deleted), `events.jsonl` schema v1, `devflow logs [--follow]`, multi-phase `status`/`recover`. Legacy `state.json`/`cron-instructions.json` migrate/read-compat on first touch. Checkout-lock acquisition in the hook path is fail-soft (warn + proceed unserialized after 120s) — a wedged sibling must not abort an advance; integrate paths fail hard instead. Validated: 252 tests, clippy/fmt clean, live two-phase e2e with both Ship gates open concurrently and both version-bump tags landing. |
| 2026-07-16 | **Phase 14 split — Hermes work (14c–e) moved to new Phase 16:** the 2026-07-14 move of Hermes into 14 was a workload-balance call made before CR-03 was deferred there (2026-07-15), which made 14 the heaviest phase instead of the slimmest. Phase 14 is now Parallel Safety + Observability, ordered 14a (CR-03) → 14b (capture_agent_output sync-path) → 14c (observability) because per-phase state files dictate what `status`/`logs`/`events.jsonl` enumerate — building observability first would mean rebuilding it. Phase 16 (Hermes Support) sits after Phase 15 so personal-infrastructure work doesn't gate OSS readiness; it depends on 14's `events.jsonl` and 13's notify hook. Dir renamed: `14-observability-hermes` → `14-parallel-safety-observability`; new `16-hermes-support` (neither 14 nor 16 had plans yet). |
| 2026-07-15 | **CR-03 follow-up deferred to Phase 14:** per-phase locks are correct, but `state.json` and main-checkout git ops stayed project-global, so `devflow parallel` remains unsafe. Fix shape (per-phase state files, phase-threaded monitor advance, coarse lock for checkout mutations) + acceptance criteria in `phases/13-mvp-core-loop/13-DEFERRED-CR-03.md`. |
| 2026-06-19 | **v2.0.0 architecture:** DevFlow is a GSD-native execution engine with gate file protocol. Two modes (full auto, supervise). State machine: Define→Plan→Code→Validate→Ship. All skip logic removed. Conventional commits permanently deprecated. |
| 2026-06-19 | **Versioning:** Hybrid Git-Based SemVer. MAJOR from project version file. MINOR = git tag count. PATCH = commit count since last minor tag. Zero human decisions per release. |
| 2026-06-19 | **Config eliminated:** No `.devflow.yaml` needed. Hardcode git-flow (main/develop/feature/), auto-detect version file, CLI flag for mode. |
| 2026-06-19 | **Hermes Plugin (Phase 13):** First-class DevFlow session mode — prevents prompt confusion, handles gate responses, surfaces state. |
| 2026-06-19 | **Phase reorganization:** Phase 10 shipped. Phase 11 refactors architecture. Bootstrap pushed to Phase 12. OSS + Hermes plugin to Phase 13. |
| 2026-06-19 | Phase 11: Git-flow — `devflow finish` (feature→develop), `devflow release` (release→main+tag), guard rails (`git_flow.enforce`). Merged into new Ship stage. |
| 2026-06-17 | Use GSD for project management going forward |
| 2026-07-08 | External code review (verified against codebase before scoping): confirmed README/ARCHITECTURE describe the pre-Phase-11 product, agent prompts are hardcoded to GSD slash commands, completion protocol conflates "stage ran" with "stage verdict," and defaults (7-day silent gate timeout, worktree opt-in with permission bypass always on) favor a personal setup over general use. Routed to Phase 13 (docs accuracy) and new Phase 14 (reliability/observability). |
| 2026-07-08 | **Reconsidering "Config eliminated" (2026-06-19):** open to reintroducing a `devflow.toml` (agent-agnostic stage/command templates, branch model) per review feedback, but deliberately **shelved** — not part of Phase 13 or 14. Revisit as its own phase when picked up. |
| 2026-07-14 | **Hermes support moved 15 → 14:** all Hermes work (HermesAgent adapter, skill-file rewrite, Hermes plugin) moved from Phase 15 to Phase 14 (retitled "Observability + Hermes Support"). Rationale: workload balance (14 was the slimmest phase) and synergy — the plugin's gate watcher and status display consume 14's `events.jsonl` and 13's notify hook, so building them in the same phase removes a cross-phase integration seam. Phase 15 retitled "OSS Readiness"; keeps Antigravity adapter, docs, dev container, contributing, crates.io publish. Dirs renamed: `14-reliability-observability-hardening` → `14-observability-hermes`, `15-oss-hermes-plugin` → `15-oss-readiness` (neither had plans yet). |
| 2026-07-14 | **MVP restructure:** priority shifted to getting the core loop (Define→Plan→Code→Validate→Ship) working end-to-end so DevFlow can be dogfooded on real projects again. Operator-confirmed scope: agents = Claude + Codex (Hermes/Antigravity deferred); gates answered via pluggable notify hook (ntfy/desktop), not terminal babysitting or Hermes plugin; MVP includes the automated Ship stage. Phase 13 repurposed as **MVP Core Loop** — claims the previously unclaimed `ship.rs` GSD-native rewrite (11h-1…4) and absorbs old-14's verdict-vs-ran split, native envelope parsing, WR-11 silent-halt fix, notify hook + configurable gate timeout, and worktree-by-default; exit criterion is a real dogfood run incl. the Full-Ship verification left BLOCKED in 12-12. Old Phase 13 (OSS + Hermes plugin) renumbered to **Phase 15** unchanged (+ actual crates.io publish). Phase 14 rescoped to pure observability (`logs`/`events.jsonl`/`status`) and now claims the previously unclaimed `capture_agent_output()` sync-path decision — both flagged-unclaimed items from the 2026-07-08 audit are now assigned. |
| 2026-07-08 | **Phase 11 closeout audit:** reviewed `11-REVIEW.md`/`11-VALIDATION.md`/`11r-VALIDATION.md`. All 5 CRITICAL findings confirmed fixed and verified. All 11 WARNING + 5 INFO findings confirmed still open in current code (spot-checked directly, none touched since Phase 11 shipped) — these were explicitly deferred to Phase 12 by `11r-CONTEXT.md` and are now scoped there (12d/12e), plus 9 untested orchestration paths (12f) and 4 never-executed manual verifications (12g) from `11-VALIDATION.md`. Two items routed to their overlapping phase instead of 12: WR-11 → Phase 14, IN-01 → Phase 13. Two items (`ship.rs` GSD-native rewrite, `capture_agent_output()` sync-path decision) remain **unclaimed by any phase** — flagged in Phase 12 CONTEXT.md, not assigned. |

- [Phase 12]: 12-09: added advance()/Ship-finish and Validate-threshold/abort terminal-path tests to close the last two 12f unit-test gaps
- [Phase 12-10]: widened shell_quote's safe set additively, documented parse_rfc3339ish's timezone-safe second-restoration, and closed the negative-UTC-offset test gap (WR-05, WR-08, 12f)
- [Phase 12]: 12-11: renamed Agent enum -> AgentKind (deleting the AgentKind=Agent alias) and adapter trait Agent -> AgentAdapter workspace-wide; removed dead State.agent_result/agent_stdout_path fields (IN-02, IN-03 closed)
- [Phase 12]: 12-12: manual-verified live Hermes gate round-trip, real Claude CLI launch+capture, and DocsUpdate fail-soft WARN visibility against the real compiled devflow binary; Full-Ship workflow recorded BLOCKED on out-of-scope ship.rs rewrite. Phase 12 (12/12 plans) complete.
- [Phase 13]: 13-01: split loop_back_to_code into prepare_loop_back_to_code (pure state mutation) + launch_stage so ReviewFailed dispatch is unit-testable without spawning the real configured agent CLI
- [Phase 13]: 13-01: non_validate_failure_fires_gate_and_hook asserts notify-hook-fired + a pure should_gate() check rather than the exact env value, since DEVFLOW_GATE_NOTIFY_CMD is process-global and races other concurrently-running gate tests
- [Phase 13]: 13-02: Made no-ship-on-Critical MANDATORY in the Ship prompt (not just review-first sequencing) so a headless run never reaches /gsd-ship's interactive optional_review step
- [Phase 13]: 13-02: Adopted the review: reason-string prefix convention (trim + case-fold) for ReviewFailed instead of a new AgentStatus enum variant, to avoid a serde-format break
- [Phase 13]: 13-03: is_error checked before DEVFLOW_RESULT marker in evaluate_layer1, so a Claude envelope's is_error: true always overrides a stale success marker
- [Phase 13]: 13-03: Codex turn.completed returns None (defers), never Success -- a marker-less turn cannot silently advance a stage
- [Phase 13]: 13-03: Layer 2 commit gate uses explicit matches!(stage, Stage::Plan | Stage::Code), not is_agent_stage(), since is_agent_stage() also includes Define
- [Phase 13]: 13-04: Retained --worktree as a hidden deprecated no-op alias for one release instead of removing it, per cross-AI review consensus (#6)
- [Phase 13]: 13-04: Computed effective worktree flag as !no_worktree in the Start match arm, leaving start()'s internal signature and parallel()/sequentagent() call sites unchanged
- [Phase 13]: 13-05: Verdict deserializer uses exact-case matching (not case-folding) per the plan's explicit fail-safe test contract.
- [Phase 13]: 13-05: Excluded Stage::Validate from the generic single-command-template prompt test (renamed it) since Validate now has its own dedicated verdict-requiring prompt, mirroring Ship's existing special-case exclusion.
- [Phase 15]: 15-01: SECURITY.md Supported Versions (v1.0.0+) already covers Cargo.toml 1.2.0 — left unchanged; DEPENDENCIES.md's "Required for Shipping" header also dropped the phantom `devflow ship` command (alongside the plan-flagged `devflow confirm`) in favor of the real gate-driven Ship flow (`devflow gate approve <phase> --stage ship`)
- [Phase 15]: 15-02: ARCHITECTURE.md full rewrite also corrected the Agent model (trait renamed `Agent`->`AgentAdapter` in 12-11; prompts are per-stage via `prompt.rs::stage_prompt`, not one shared template) and Completion evaluation's Layer 2 commit gate (scoped to Plan/Code only, not every stage) — both classified "already accurate" by 15-PATTERNS.md but found stale on direct source verification; CONTRIBUTING.md's "Adding a New Agent" section left untouched (out of files_modified scope) despite already duplicating the checklist inline with the stale trait name — flagged for a future cleanup
- [Phase 15]: 15-03: Verified devcontainer base image tag live against registry (2.0.13-1-bookworm, not stale illustrative 1-1-bookworm) and pinned devcontainers/ci action to @v0.3; CODE_OF_CONDUCT.md spot-checked and left unmodified (contact wording current)
- [Phase 15]: 15-04: Sourced canonical Apache-2.0 body from an already-vendored copy in the local Cargo registry cache (byte-diffed) after an initial from-memory reconstruction was self-caught with garbled Section 8/9 text; kept dual license per plan's locked resolution
- [Phase 16]: 16-01: absent feature branches are treated as already merged so terminal retries are safe after feature_finish deletes the branch
- [Phase 16]: 16-01: merge_result telemetry separates actual merge effects from successful no-op hook execution
- [Phase 17]: 17-01: typed-outcome taxonomy + fail-closed policy table — ResourceKilled/AgentUnavailable, as_wire_str(), outcome_policy::decide_action, State.infra_failures/MAX_INFRA_FAILURES
- [Phase 17]: 17-02: first workspace build.rs — resolves git-common-dir via `git rev-parse --git-common-dir` from CARGO_MANIFEST_DIR (not a relative `.git/HEAD`) and emits absolute rerun-if-changed paths for HEAD/refs/packed-refs; DEVFLOW_BUILD_COMMIT/DIRTY/TIMESTAMP via cargo:rustc-env, degrading gracefully with no git
- [Phase 17]: 17-03: evaluate_layer3 zero-commit/no-declaration reclassified Unknown->Failed (D-02/D-03 case 3, human review flag); commits-present stays Unknown for Plan 04's gate. evaluate_layer0 now runs every stage (not just Code) and returns affirmative Success when all approved declared probes pass even at zero commits; PLAN discovery now reads project_root while probe execution keeps execution_root (fixes a worktree PLAN-removed false veto pre-existing since 16-01).
- [Phase 17]: 17-04: advance() dispatches exhaustively on outcome_policy::decide_action (Unknown/Failed/RateLimited/ResourceKilled/AgentUnavailable each gate/resume/abort, never silently advance); GateInfra path (handle_infra_outcome) bumps infra_failures on every stage incl. Validate/Ship, never consecutive_failures; new devflow resume --phase N relaunches saved state (no State::new/branch/worktree reset) as the safe rate-limit auto-resume target; advance_evaluated now emits decided_by_layer + AgentStatus::as_wire_str()
- [Phase 17]: 17-05: preflight_interactivity_check scoped to AgentKind::Codex only (not every adapter) — a blanket check broke 3 passing start() integration tests since Claude/OpenCode complete Define headlessly; launch_stage signature changed to &mut State so run_preflight/enforce_build_staleness can drive run_gate
- [Phase 17]: 17-06: infra_failures reset scoped to transition() (forward-stage-transition path) only, not gate-driven retry branches — MAX_INFRA_FAILURES bounds a stuck loop across forward progress, not every same-stage retry
- [Phase 17]: 17-08: run_preflight returns Result<bool, CliError> to disambiguate 'preflight passed' from 'a resolved gate already relaunched everything' (CR-01 double-agent-spawn fix, GAP-1 closed, nyquist_compliant: true); regression tests inject a Cell<bool> FailOnceAdapter directly into run_preflight and stub PATH under ENV_MUTEX so a real, completing launch_stage never risks spawning a real agent CLI
- [Phase 17]: 17-09: GAP-2 (concurrent_ship_advances_finish_both_phases_independently unbounded wedge) resolved test-level: DEVFLOW_GATE_TIMEOUT_SECS bounded to 2s under ENV_MUTEX for the reopened loser gate's poll only, 7-day production default untouched. RED reproduced the hang under 120s external timeout; debug instrumentation caught both phases computing the identical version tag ~1.8ms apart, proving the checkout lock occasionally fails to fully serialize the two threads' terminal hooks -- recorded as an explicit OUT-OF-SCOPE product-level version-tag contention question for future ship/version-bump concurrency work, not fixed here. 25 consecutive isolated runs: 0 hangs, 9 hit the race and resolved via the bounded path.
- [Phase 17]: 17-11: CR-02 resolved -- build.rs always reruns via an unfingerprintable sentinel, DEVFLOW_BUILD_TIMESTAMP removed entirely, staleness's second signal replaced by a (build_dirty, tree_has_modified_build_inputs) decision table (Stale when built clean and now dirty; Indeterminate, never blocking, when built dirty and still dirty)
- [Phase 17]: 17-12: WR-04 resolved -- ChangelogAppend reordered strictly after VersionBump in hooks_after_ship() (removed from the Validate->Ship transition), reads version::read_version (new, git-free) instead of compute_version to avoid deriving a version one higher than the tag VersionBump just cut, and commits its own write via a new GitFlow::commit_path; version_bump had the identical uncommitted-write defect on its own version-file write and is fixed the same way
- [Phase 17]: 17-13: GAP-6/GAP-7 closed via write_version remainder-preservation fix and HookContext.shipped_version threading; row 12 restored to green
- [Phase 18]: 18-01: 18a doctor project-aware reconciliation -- pure PhaseFacts/PhaseFinding/reconcile_phase core (5 named checks: gate-pending-without-gate, orphan-gate, dead-agent, stage/event drift, missing branch) wired into doctor()'s text and --json output via collect_phase_facts/render_reconciliation; proven read-only by a twice-run fixture asserting state-file size/mtime and events.jsonl line count are unchanged
- [Phase 18]: 18-02: WR-03 test stabilization -- `parallel_creates_two_worktrees_and_spawns_two_monitors` asserts each stdout capture inside its own `wait_for` window (mirrors `wait_for_pid`'s already-fixed archive-timing pattern); plan's literal combined-assertion instruction was itself racy (25x loop reproduced it at run 15/25), corrected to interleaved per-wait assertions matching the plan's own must_haves.truths
- [Phase 18]: 18-03: monitor liveness (18b) — State.monitor_pid persisted at spawn (launch_stage re-saves after spawn_monitor, since transition() saves before launch_stage runs), pure liveness() predicate (None-first match so an unrecorded monitor can never render Stuck) shared verbatim by devflow status's new monitor row and doctor's new check_dead_monitor finding, spliced into reconcile_phase immediately after check_dead_agent per 18-01's extend-not-reorder contract. Manually verified end-to-end against a synthetic dead-monitor fixture: status prints stuck — needs devflow resume, doctor prints a matching finding with a devflow resume --phase N repair, neither leaks a filesystem path or username (WR-02 class).
- [Phase 18]: 18-04: transition_resets_consecutive_failures added as a pure mode.rs predicate (not a Mode method) resolving Open Question 1 -- false only for (Code, Validate), making MAX_CONSECUTIVE_FAILURES reachable; infra_failures' unconditional reset is untouched (transition_resets_infra_failures passes byte-for-byte unchanged); handle_validate_outcome's increment switched to saturating_add to close the overflow-wrap reintroduction risk
- [Phase 18]: 18-05: classify_validate_outcome checks Some(Verdict::Pass) first (ordinary Validate verdict:pass still advances directly, unchanged from pre-18e); the combined 18d+18e test is one #[test] fn calling two ~30-line helpers to satisfy both the exact-name acceptance criterion and the function-length convention; ValidateOutcome::Ambiguous's final match arm is unreachable!() rather than silently folded into Failed, since forced=true always returns via the gate branch above
- [Phase 18]: 18-06: enforce_build_staleness derives execution_root = state.worktree_path.unwrap_or(project_root); is_self_dogfood_workspace stays project_root-scoped (Assumption A3)
- [Phase 18]: 18-07: launch_stage split into launch_stage (resolution + run_preflight guard) + launch_stage_inner (everything after); run_preflight's Advance arm calls launch_stage_inner directly (skip), LoopBack still calls full launch_stage (re-check), either bounded by persisted State.preflight_retries / mode::MAX_PREFLIGHT_RETRIES=3 checked before any new gate is written; counter resets to 0 (persisted) on preflight pass and human Advance. Phase 18 (18a-18g) complete.
- [Phase 18]: 18-07: AlwaysFailAdapter cannot reproduce a preflight failure that survives a relaunch (launch_stage always re-resolves the REAL production adapter via agents::adapter_for, discarding whatever was passed into the outer run_preflight call) -- used preflight_interactivity_check (a pure function of state) as the deterministic wedge-reproduction trigger for the three new tests instead; verified empirically both ways (unfixed code + literal plan setup = no observable difference; unfixed code + interactivity-check setup = reproduces the exact documented wedge).
- [Phase 19]: 19-01: ensure_devflow_dir(dir) returns std::io::Result (not a crate error enum) so ? converts at all 7 sites across 6 error enums with zero signature churn; marker resolution walks dir.components() (not ancestors()) so relative .devflow-leaf paths resolve correctly
- [Phase 19]: 19-02: exe_path in workflow_started_payload redacted via .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())) (WR-02); to_string_lossy (not to_str) keeps non-UTF-8 names as a string, preserving null as the distinct "could not resolve" signal; worktree field's own full-path exposure (T-19-09) left untouched and surfaced, not fixed, per D-15's scope
- [Phase 19]: 19-03: commit_path no longer passes --allow-empty, closing 19b/D-16 (a repeat call with unchanged content now creates zero commits, not a forced empty one); discovered mid-fix that git's "nothing to commit" message is on stdout not stderr, so the plan's literal reuse of git_raw's stderr-only error mapping could never surface it -- added a sibling git_raw_combined helper (stdout+stderr) used only by commit_path, leaving git_raw's own error-mapping branch and commit_all byte-identical; git_raw and git_raw_combined both pin LC_ALL=C/LANG=C (T-19-14); D-17 finding: commit_all's empty-commit behavior is not load-bearing (its only caller, docs_update, already treats a commit failure as non-fatal, and no test asserts a commit exists after it runs) -- commit_all left unmodified regardless
- [Phase 19]: 19-04: ai-change-acceptance project skill (D-19's 5 requirements + 4 rejection patterns) plus a CONTRIBUTING.md section, wired into /gsd-code-review per D-18; also fixed .gitignore's blanket .claude/ ignore (carved out !.claude/skills/**, mirroring the existing .codex/* pattern) since it would otherwise have silently prevented the new skill from ever being committed
- [Phase 19]: 19-06: no D-12 ENV_MUTEX finding — three consecutive cargo test -p devflow runs post-hoist stable, 0 failed each time
- [Phase 19]: 19-05: dogfood checkpoint for 19g approved on combined evidence — an in-session five-diff run (same agent authored and reviewed, so it could not prove isolated wiring by itself) plus an independently-spawned, context-isolated gsd-code-reviewer subagent that caught both anti-pattern shapes cold. Recorded gap for later triage: the isolated reviewer's generic judgment agreed but did not cite the ai-change-acceptance contract by name unless the dispatch explicitly loaded it — review-dispatch prompts should load the skill explicitly, and part of that wiring surface lives outside this repository.
- [Phase 19]: 19-07: staleness + preflight clusters extracted into staleness.rs/preflight.rs (shakedown run for the mechanical extraction procedure ahead of 19-08/19-09's larger clusters); 438/438 tests unchanged, every moved function diffs clean against the 19-06 baseline SHA modulo pub(crate); preflight <-> pipeline bidirectional call preserved as direct calls, not abstracted. Two findings recorded for 19-08/19-09: a wider-than-estimated pub(crate) surface (worktree_writable_roots, ensure_agent_binary, agent_program, phase_artifact_on_develop all needed it beyond the plan's run_preflight/launch_stage_inner estimate), and a bug in the plan's own literal name-set extraction command (`rg '::tests::' | sed 's/.*::tests:://'` silently drops main.rs's own top-level mod tests entries; corrected to `sed 's/.*:://'`).
- [Phase 19]: 19-08: pipeline state machine split into pipeline_launch.rs/pipeline_outcomes.rs/pipeline_gate.rs (D-06 seams A/B/C), main.rs down to 3,313 lines from phase-start 8,467; three-way module cycle preserved as direct pub(crate) calls, zero unexplained diffs against baseline

## Roadmap Evolution

- Phase 18 reprioritized, fixed Phase 19 eliminated (2026-07-20): Dogfood Reliability Hardening (dir `18-dogfood-reliability-hardening`) takes Phase 18's slot from Hermes Support (dir renamed to `999.1-hermes-support`); the fixed "Phase 19: Operator Observability" entry is replaced entirely — its content is absorbed into 18, confirmed already fixed, or moved to backlog dirs `999.2`–`999.5`. See 2026-07-20 decision entry.
- Phase 14 split (2026-07-16): Hermes work (adapter, skill rewrite, plugin) moved out of 14 to new Phase 16 (`16-hermes-support`); 14 retitled Parallel Safety + Observability (dir `14-parallel-safety-observability`), leading with the deferred CR-03 flaw. See 2026-07-16 decision entry.
- MVP restructure (2026-07-14): Phase 13 repurposed as MVP Core Loop (dir `13-mvp-core-loop`); old Phase 13 OSS/Hermes content moved to new Phase 15; Phase 14 rescoped to observability. Later same day: Hermes work moved 15 → 14 (now `14-observability-hermes`), 15 slimmed to OSS Readiness (`15-oss-readiness`). See 2026-07-14 decision entries.
- Phase 14 added: Reliability & Observability Hardening — verdict-vs-ran split in completion protocol, native per-agent JSON envelope parsing, worktree-isolation-by-default for `start`, observability (`devflow logs`, `events.jsonl`, gate notify hook, configurable gate timeout). Scoped from external code review (2026-07-08). Extended 2026-07-08 with WR-11 (silent halt on non-Validate stage failure, from Phase 11 code review).
- Phase 13 scope extended: ARCHITECTURE.md full rewrite, `.devflow.yaml` decoy removal, `--help` snapshot CI test, Hermes skill file rewrite — added to existing 13b alongside the already-scoped README rewrite. Extended 2026-07-08 with IN-01 (stale lib.rs rustdoc, from Phase 11 code review).
- Phase 12 scope extended: publish `devflow` to crates.io (name confirmed available 2026-07-08). Fully scoped 2026-07-08 (CONTEXT.md written): bootstrap/versioning/crates.io plus Phase 11's deferred code-review debt (WR-01–10, IN-02–05), test coverage gaps, and never-executed manual verifications.
- Phase 12 and 13 given full `### Phase N:` sections in ROADMAP.md (2026-07-08) — previously only table rows, which meant `gsd-tools roadmap.analyze` could not see them as active phases (a real forensic gap found during `/gsd-progress --forensic`).

## Performance Metrics

| Phase | Plan | Duration | Notes |
|-------|------|----------|-------|
| Phase 12-bootstrap-housekeeping P09 | 15min | 2 tasks | 1 files |
| Phase 12-bootstrap-housekeeping P10 | 15min | 3 tasks | 1 files |
| Phase 12-bootstrap-housekeeping P11 | 15min | 2 tasks | 13 files |
| Phase 12-bootstrap-housekeeping P12 | n/a | 3 tasks | 0 files |
| Phase 13-mvp-core-loop P01 | 17min | 3 tasks | 3 files |
| Phase 13-mvp-core-loop P02 | 10min | 2 tasks | 2 files |
| Phase 13-mvp-core-loop P03 | 12min | 3 tasks | 1 files |
| Phase 13-mvp-core-loop P04 | 7min | 2 tasks | 2 files |
| Phase 13-mvp-core-loop P05 | 15min | 2 tasks | 3 files |
**Per-Plan Metrics:**

| Plan | Duration | Tasks | Files |
|------|----------|-------|-------|
| Phase 15-oss-readiness P01 | 15min | 3 tasks | 3 files |
| Phase 15-oss-readiness P02 | 40min | 3 tasks | 3 files |
| Phase 15-oss-readiness P03 | 10min | 3 tasks | 3 files |
| Phase 15-oss-readiness P04 | 12min | 2 tasks | 1 files |
| Phase 16-pipeline-reliability-hardening P01 | 5min | 3 tasks | 3 files |
| Phase 16 P02 | 23min | 3 tasks | 5 files |
| Phase 16 P03 | 4min | 2 tasks | 5 files |
| Phase 16 P04 | 2min | 2 tasks | 2 files |
| Phase 16 P05 | 8min | 2 tasks | 14 files |
| Phase 16 P06 | 3min | 2 tasks | 2 files |
| Phase 16 P07 | 4min | 2 tasks | 7 files |
| Phase 17 P01 | 15min | 2 tasks | 6 files |
| Phase 17 P02 | 2min | 2 tasks | 2 files |
| Phase 17-pipeline-dogfood-followup P03 | 5min | 2 tasks | 1 files |
| Phase 17-pipeline-dogfood-followup P04 | 25min | 2 tasks | 4 files |
| Phase 17-pipeline-dogfood-followup P05 | 45min | 2 tasks | 2 files |
| Phase 17-pipeline-dogfood-followup P06 | 25min | 3 tasks | 5 files |
| Phase 17-pipeline-dogfood-followup P08 | 20min | 3 tasks | 3 files |
| Phase 17-pipeline-dogfood-followup P09 | 50min | 2 tasks | 2 files |
| Phase 17-pipeline-dogfood-followup P11 | 40min | 3 tasks | 4 files |
| Phase 17-pipeline-dogfood-followup P12 | 20min | 3 tasks | 5 files |
| Phase 17-pipeline-dogfood-followup P13 | 15min | 3 tasks | 4 files |
| Phase 18-dogfood-reliability-hardening P01 | 35min | 2 tasks | 1 files |
| Phase 18 P02 | 15min | 2 tasks | 1 files |
| Phase 18 P03 | 30min | 3 tasks | 3 files |
| Phase 18 P04 | 35min | 2 tasks | 2 files |
| Phase 18 P05 | 50min | 3 tasks | 2 files |
| Phase 18 P06 | 21min | 2 tasks | 1 files |
| Phase 18 P07 | 25min | 3 tasks | 4 files |
| Phase 19-release-integrity-main-rs-decomposition P01 | 55min | 3 tasks | 8 files |
| Phase 19-release-integrity-main-rs-decomposition P02 | 12min | 1 tasks | 1 files |
| Phase 19 P03 | 20min | 2 tasks | 1 files |
| Phase 19-release-integrity-main-rs-decomposition P04 | 20min | 2 tasks | 5 files |
| Phase 19 P06 | 22min | 3 tasks | 6 files |
| Phase 19 P05 | n/a | 1 tasks | 0 files |
| Phase 19 P07 | 71min | 2 tasks | 3 files |
| Phase 19 P08 | 37min | 3 tasks | 5 files |

## Session

**Last session:** 2026-07-23T17:20:24.303Z
**Stopped at:** Phase 21 context gathered
**Resume file:** .planning/phases/21-operator-usability-release-execution/21-CONTEXT.md
