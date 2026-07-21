# Phase 19: Release Integrity + `main.rs` Decomposition - Context

**Gathered:** 2026-07-21
**Status:** Ready for planning

<domain>
## Phase Boundary

Close the two release-integrity defects whose blast radius reaches outside this
repository, then decompose `crates/devflow-cli/src/main.rs` (8,467 lines) as a
**pure-move refactor with zero behavioral change**. Adds an AI change acceptance
contract on a parallel, source-conflict-free track.

**Targets v1.6.0.** Nothing here is breaking and — apart from the PII fix —
almost nothing is user-visible. The v2.0.0 milestone closes at Phase 20.

Four units, promoted from backlog 2026-07-21:

| Unit | Was | Pri | Size | Linear |
|---|---|---|---|---|
| 19a | 999.10 `.devflow/` artifact hygiene | Urgent | S | DEN-35 |
| 19b | 999.11 `commit_path` empty commits | High | S | DEN-36 |
| 19c–19f | 999.8 split `main.rs` | High | L | DEN-33 |
| 19g | 999.16 AI change acceptance contract | High | M | DEN-41 |

**Out of scope:** new CLI capability (`--until`, manual ship override, `gate
show`), release-cut automation, and any behavioral change bundled into the
split. Those are Phase 20 (v2.0.0).

</domain>

<decisions>
## Implementation Decisions

### Env serialization across the split (the phase's principal risk)

- **D-01: A single shared `ENV_MUTEX` must survive the split.** Hoist it into a
  shared test-support module that every split module imports. Preserves today's
  exact guarantee with no per-module reasoning about which variable lives where.
  Smallest change consistent with "pure move."
- **D-02: Rejected — per-module mutexes.** Scouting proved this unsafe here:
  `PATH` is mutated **36 times across 12 separate lock regions spanning lines
  4522–6795**, which map onto at least three target clusters (preflight,
  staleness, pipeline). Per-module mutexes would silently break `PATH`
  serialization — and `PATH` is exactly what 19i raced on when it hit 2/2 in CI.
- **D-03: Rejected — eliminating env mutation via dependency injection.** It
  would remove the failure class permanently, but it is a behavioral change to
  106 tests, which destroys the equivalence proof that makes this refactor safe.
  Record as a deferred idea, do not do it here.
- **D-04: The invariant to document is not "one mutex."** It is *"every env var
  is guarded by exactly one mutex, and no var is touched under two."* State this
  explicitly wherever the shared mutex lands — it currently holds by accident
  and is enforced nowhere.

### Module layout

- **D-05: Flat sibling modules, all staying in `devflow-cli`.** `preflight.rs`,
  `staleness.rs`, `commands.rs`, `parallel.rs`, `config_parse.rs`, plus a thin
  `main.rs` retaining only `main`, `run`, and arg routing.
- **D-06: Split the pipeline state machine further — it is the actual
  bottleneck.** Measured at HEAD: pipeline holds ~1,040 lines and absorbed **3
  of Phase 18's 7 plans** (18-04, 18-05, 18-07), more than any other cluster.
  Natural seams: `launch_stage`/`advance`/`transition`, the `handle_*_outcome`
  family, and `run_gate`/`finish_workflow`. This is what takes Phase 18's shape
  from 3 waves to 2.
- **D-07: Rejected — `commands/` per-subcommand subdirectory.** Measured
  against Phase 18's real workload it buys **zero** wave reduction: commands
  absorbed only 2 plans (18-01 doctor, 18-03 status+liveness) and those two
  would still collide on `doctor.rs` anyway, while `pipeline.rs` would remain a
  3-plan serial cluster. It also adds ~10 files of move surface, and shared
  display helpers tend to collect into a `commands/common.rs` that
  re-centralizes the contention the split was meant to remove. *(Note: the
  "inconsistent with the codebase" objection was withdrawn — `agents/mod.rs` +
  `claude.rs`/`codex.rs` is exactly this pattern.)*
- **D-08: Do NOT move `staleness`/`preflight` into `devflow-core` this phase.**
  Operator confirmed the public-API change is not itself a cost (no external
  consumers). The real cost is touching two crates' module trees inside a
  refactor whose safety rests on minimal change. Cheap follow-up once the split
  is proven.

### Split execution and verification

- **D-09: Pure move, zero behavioral change.** No logic edits bundled in — that
  property is what makes the existing suite a valid equivalence proof.
- **D-10: Split the test module in the same operation.** Leaving 4,442 test
  lines behind defeats the purpose. Rust unit tests reach parent-module private
  items, so each cluster's tests move with its code; tests spanning clusters
  need explicit handling.
- **D-11: Verify on a branch with CI. Local-green is explicitly insufficient.**
  19i is direct evidence that CI's shared runners widen race windows relative to
  this workstation (hit 2/2 in CI after passing locally most of the time).
- **D-12: A partially-completed split with the `ENV_MUTEX` question surfaced is
  an acceptable outcome.** If serialization cannot be preserved without a
  structural change to how tests serialize env mutation, that is a finding to
  raise — not something to patch around silently mid-refactor.

### 19a — `.devflow/` artifact hygiene

- **D-13: Apply both fixes; either alone leaves half the problem open.** WR-02
  stops the sensitive data being written; WR-01 stops the file being published.
- **D-14 (CORRECTED 2026-07-21 at plan time — original premise was false):**
  WR-01 is fixed by a **new** `workflow::ensure_devflow_dir()` that creates the
  directory and writes a `.devflow/.gitignore` containing `*`, with **all 7
  existing `create_dir_all` sites converted to call it**, plus a **regression
  test proving every `.devflow/`-writing path produces the `.gitignore`**.
  Self-ignoring, needs no change to the user's root `.gitignore`, closes the
  hole for every constructor, and the test stops the next new writer silently
  reopening it. Rejected the narrower alternative (scoping `docs_update`
  through `commit_path`) — it fixes one call site and leaves `.devflow/`
  committable.

  **Why corrected.** The original decision named `lock::ensure_devflow_dir`,
  taken from the backlog dossier. **That function does not exist** — zero
  matches in `crates/`, confirmed at HEAD. `.devflow/` is instead created from
  **7 independent `create_dir_all` sites**: `workflow.rs:95`, `gates.rs:325`,
  `monitor.rs:98`, `agent_result.rs:964`, `events.rs:58`, `ship.rs:85`,
  `lock.rs:82`.

  **Why not `save_state` (19-RESEARCH.md's proposed chokepoint).** Verified
  false at plan time: `run_agent_blocking` (`main.rs:2417`) — the
  `sequentagent`/`parallel` path — writes `.devflow/` content via
  `archive_phase_files` and `spawn_monitor_no_advance` on state the source
  itself calls *"Synthetic, never-persisted state… sequentagent does not
  participate in the stage machine."* It never calls `save_state`, so that
  chokepoint would leave the entire parallel path leaking. This was
  RESEARCH.md's own open question #3; the grep was run and the assumption
  (A2) does not hold.

  **Also reconcile:** `devflow_dir()` is defined **twice** —
  `workflow.rs:33` (public, 7 uses) and `agent_result.rs:872` (private
  duplicate, 9 uses). Any single chokepoint must account for both.

  **Rejected — making `workflow::devflow_dir()` itself create and write.**
  Fewest call-site edits, but it turns a pure path accessor called 17 times
  (including read-only paths like `doctor`/`status` and in tests) into a
  side-effecting function — exactly the class of behavioral change this phase
  exists to avoid.
- **D-15: WR-02 — emit only `current_exe().file_name()`, not the full path.**
  Keeps a useful binary-name signal; `DEVFLOW_BUILD_COMMIT`/`DEVFLOW_BUILD_DIRTY`
  already carry the real diagnostic value. Rejected dropping the field entirely
  (changes the `events.jsonl` schema for anything parsing it).

### 19b — `commit_path` empty commits

- **D-16: Drop `--allow-empty` from `commit_path` and let the existing
  `nothing to commit` arm become the genuine no-op.** Restores the doc comment's
  stated contract and revives what is currently dead code.
- **D-17: `commit_all` at `git.rs:312` is OUT of scope — but check and record
  why.** Its empty-commit behavior may be load-bearing somewhere. Determine the
  answer and write it down; do not change it in the same pass as a
  release-integrity fix. It is also the call site 19a modifies, so changing it
  here would collide the two units.

### 19g — AI change acceptance contract

- **D-18: Wire the contract into `/gsd-code-review`'s actual criteria, plus
  prose in `CONTRIBUTING.md`.** The review already runs before Ship and already
  refuses to ship on Critical findings — that is where enforcement exists.
  Rejected a net-new mechanical lint pass: it is additional tooling in a phase
  already carrying an L-sized refactor.
- **D-19: The five requirements** — a regression test that fails before the
  change; at least one assertion at a public/stable boundary; evidence the test
  fails for the intended reason; full affected-package tests + clippy
  `-D warnings` + fmt; independent review of both implementation and test
  signal. Reject tests that only assert constants, reproduce the production
  algorithm, compare a function call with itself, or grep implementation text
  without a runtime contract.

### Sequencing

- **D-20: 19a and 19b land BEFORE the split.** They stay small diffs against the
  file everyone knows rather than against seven new modules.
- **D-21: 19g has zero source overlap and may run in any wave.**

### Claude's Discretion

- Exact file names and the precise cut points for the pipeline sub-split —
  D-06 names the seams, but the final boundaries should follow what the code
  actually shows at plan time.
- Where the shared test-support module lives and what it is called.
- Whether `config_parse` is large enough to warrant its own file or folds into
  the thin `main.rs`.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase source material
- `.planning/phases/19-release-integrity-main-rs-decomposition/19-BACKLOG-DOSSIER.md` —
  the consolidated promotion dossier. Full verbatim text of all four original
  backlog items (999.10, 999.11, 999.8, 999.16) with per-unit detail,
  reproduction steps, and re-verification notes. **Read this before planning.**
- `.planning/ROADMAP.md` § "Phase 19" and § "Phase 19 scoping (2026-07-21)" —
  phase entry, sequencing rationale, and the milestone-label correction.

### Findings this phase closes
- `.planning/phases/17-pipeline-dogfood-followup/17-REVIEW.md` — WR-01, WR-02
  (unit 19a) and WR-03 (unit 19b) as originally written.
- `.planning/TEST-SUITE-QA-REVIEW.md` — Codex's 2026-07-21 test-suite QA pass;
  P0 recommendation #2 is unit 19g. Also the source of the
  `ReviewerSetTestAdapter` example.

### Codebase maps — NOTE: two are stale, see D-22 below
- `.planning/codebase/STRUCTURE.md` — module layout and "where to add new code."
  **STALE:** still describes `main.rs` as a "single 1000+ line file."
- `.planning/codebase/TESTING.md` — test organization and conventions.
  **STALE:** still cites the deleted `devflow_ignores_stray_devflow_yaml` as its
  example test for `main.rs`.
- `.planning/codebase/CONVENTIONS.md`, `.planning/codebase/CONCERNS.md` —
  not verified stale, but written 2026-07-17 (pre-Phase-18).

### Operator-facing docs affected
- `docs/OPERATIONS.md` — advertises `events.jsonl` as a file to tail and paste,
  which is what makes WR-02 a real exposure rather than a theoretical one.
- `docs/CONTRIBUTING.md` — receives the 19g prose.

</canonical_refs>

<code_context>
## Existing Code Insights

### Verified at HEAD 2026-07-21 (all four source claims still hold)

- `crates/devflow-core/src/hooks.rs:184` — `git.commit_all("docs: update
  generated docs")`, still the **only remaining `commit_all` caller**.
- `crates/devflow-cli/src/main.rs:902` — `exe_path` emission. *(Drifted from
  `:843` as recorded in the backlog item.)* Its test assertion is at
  `main.rs:6879`.
- `crates/devflow-core/src/git.rs:312` (`commit_all`) and `:336`
  (`commit_path`) — both `--allow-empty` sites present.
- `crates/devflow-core/src/git.rs:659` — test comment on "the property that
  distinguishes `commit_path` from `commit_all`." Check whether existing tests
  pin the empty-commit behavior; fixing 19b changes observable output.

### `main.rs` measured at HEAD — the backlog item's figures were stale

**8,467 lines** = 4,025 production + a 4,442-line `#[cfg(test)]` module starting
at `:4026`, containing **106 tests**. It is **3.4x** the next largest file
(`agent_result.rs`, 2,505). The item recorded 6,239 lines — the file has grown
**+35%** since. **All cluster line ranges in the promotion dossier are stale and
must be re-measured at plan time**; cluster identities are expected to hold.

Cluster sizes measured at HEAD, with Phase 18 plan contention:

| Cluster | ~Lines | Phase 18 plans |
|---|---|---|
| commands/display (`status`…`doctor_json_body`, ~2743–4025) | ~1,280 | 2 |
| **pipeline state machine** (~1254–2290) | ~1,040 | **3** ← bottleneck |
| parallel/sequentagent (~2291–2700) | ~410 | 0 |
| staleness/provenance (~936–1253) | ~320 | 1 |
| preflight | ~200 | 1 |

### `ENV_MUTEX` — the real shape of the risk

There is **no single `ENV_MUTEX`.** Three independent `static Mutex<()>`
definitions exist, each inside its own `mod tests`:

| Definition | Guards |
|---|---|
| `crates/devflow-cli/src/main.rs:4034` | `PATH` (36), `DEVFLOW_GATE_TIMEOUT_SECS` (9), `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` (2), `DEVFLOW_GATE_NOTIFY_CMD` (1) |
| `crates/devflow-core/src/gates.rs:348` | `DEVFLOW_GATE_NOTIFY_CMD` |
| `crates/devflow-core/src/config.rs:174` | `DEVFLOW_CAPTURE_RETENTION`, `DEVFLOW_REVIEW_ANGLES`, `DEVFLOW_EXTERNAL_VERIFY_ENABLED` |

They are sound today only because each guards a **disjoint** variable set — an
accident, documented nowhere. `gates.rs`'s own safety comment reasons per-*variable*
("no other thread reads/writes `DEVFLOW_GATE_NOTIFY_CMD`"), not per-mutex, which
is the correct instinct but is not enforced. `main.rs` and `gates.rs` both touch
`DEVFLOW_GATE_NOTIFY_CMD` under *different* mutexes and are safe only because
they compile into separate test binaries (different processes).

18 `.lock()` sites in `main.rs` at lines: 4240, 4507, 4889, 5234, 5561, 5626,
5700, 5817, 5896, 5933, 6534, 6595, 6684, 6757, 7073, 7104, 7150, 8003.

### Established Patterns

- **Submodule precedent exists:** `crates/devflow-core/src/agents/` is
  `mod.rs` + one file per adapter. A subdirectory layout would not be foreign —
  it was rejected on measured wave-count grounds (D-07), not style.
- **Error types are co-located with responsibility** (`GitError` in `git.rs`).
  Split modules should follow this if they carry their own error variants.
- **No `.unwrap()` in production code** — `?` and `.map_err()` throughout.
- Integration tests live in `crates/devflow-cli/tests/` (`build_provenance.rs`,
  `phase7_cli.rs`, `log_format_env.rs`, `help_snapshot.rs` + `snapshots/`,
  `gitignore_coverage.rs`, `devcontainer_ci_failfast.rs`); unit tests live in
  their module's `#[cfg(test)]` block.

### Integration Points

- `lock::ensure_devflow_dir` (`crates/devflow-core/src/lock.rs`) — 19a's WR-01
  fix lands here. Verified 2026-07-20 to contain no gitignore logic today.
- `hooks.rs:239-243` (`version_bump`) — the caller whose retry path makes 19b's
  empty commit reachable.
- Existing guards `crates/devflow-cli/tests/gitignore_coverage.rs` and
  `doc_check.rs:283` cover **only DevFlow's own repo** — neither protects a
  downstream user, which is the entire 19a problem.

</code_context>

<specifics>
## Specific Ideas

- **On the layout decision:** operator's stated priority is *maximizing
  development productivity*, and explicitly not protecting the public API
  surface (no external consumers). D-06/D-07 follow directly from that — the
  choice was made on measured wave-count evidence against Phase 18's real
  workload, not on style or API-stability grounds.
- **A partially-done split is acceptable** if `ENV_MUTEX` forces the issue
  (D-12). Do not let a planner treat "split all seven clusters" as a hard
  success criterion that overrides surfacing a real serialization finding.

</specifics>

<deferred>
## Deferred Ideas

- **Eliminate env mutation in tests via dependency injection** (from D-03) —
  removes the `ENV_MUTEX` failure class permanently instead of relocating it.
  Rejected here only because it is a behavioral change to 106 tests that would
  destroy this phase's equivalence proof. Strong candidate for its own phase
  once the split has landed, and arguably the real fix for the class behind
  19i / GAP-2 / DEN-29.
- **Move `staleness`/`preflight` into `devflow-core`** (from D-08) — cheap
  follow-up once the split is proven.
- **Enforce the one-var-one-mutex invariant mechanically** (from D-04) — a lint
  or test asserting no env var is guarded by two different mutexes. Worth doing
  after the shared-mutex hoist makes the invariant explicit.
- **Regenerate `.planning/codebase/STRUCTURE.md` and `TESTING.md`** — both are
  stale. `TESTING.md` was already owned by this phase as a mechanical
  follow-up; `STRUCTURE.md`'s staleness was found during this discussion
  (describes `main.rs` as "1000+ lines"). Do both once the split lands, since
  the split invalidates them further.
- **Phase 20 (v2.0.0):** 999.6 `--until` (DEN-31), 999.7 manual ship override
  (DEN-32), 999.13 release-cut automation (DEN-38), likely 999.3 `gate show`
  (DEN-28). All land in `main.rs`, which is why this split precedes them.

</deferred>

---

*Phase: 19-release-integrity-main-rs-decomposition*
*Context gathered: 2026-07-21*
