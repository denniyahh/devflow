# Phase 20: Release Correctness + Operator Control - Context

**Gathered:** 2026-07-22
**Status:** Ready for planning — discuss-phase complete

<domain>
## Phase Boundary

Close the two defects that make DevFlow's own release cut unreliable, then give
the operator the two controls the pipeline has never had: a clean stop point
short of Ship, and a way to drive a phase through Ship when the monitor is dead.
Adds a release-cut preflight so the manual checklist stops being the only thing
standing between a green suite and a broken publish.

Five units, promoted from backlog 2026-07-22:

| Unit | Was | Pri | Size | Linear |
|---|---|---|---|---|
| 20a | 999.24 `VersionBump` workspace member self-pins | High | S | DEN-49 |
| 20b | 999.23 `phase7_cli.rs` git fixtures unreliable under CI | High | M | DEN-48 |
| 20c | 999.6 plan-only pipeline mode (`--until <stage>`) | High | M | DEN-31 |
| 20d | 999.13 release-cut automation (`devflow release --check`) | High | L | DEN-38 |
| 20e | 999.7 manual ship override | High | L | DEN-32 |

**Out of scope:** 999.3 (CLI operator discoverability) — Low priority, L size,
and it bundles four distinct UX gaps that deserve splitting before promotion.
It is the only member of the roadmap's originally-reserved Phase 20 set that is
deliberately left behind; see "Deferred" below.

**Milestone:** the roadmap reserves v2.0.0 for this phase as the close of the
Phase 11–20 milestone. **Open — decide at ship time, not now:** nothing in these
five units is inherently breaking, and Phase 19 already declined to burn the 2.0
slot on a non-breaking changeset. If 20c/20d/20e land as pure additions, tagging
this 2.0.0 oversells it exactly the way Phase 19 refused to. Either the phase
earns a breaking change or the milestone closes at 1.7.0 and the 2.0 slot stays
unspent.

</domain>

<decisions>
## Sequencing

**Wave 1 — 20a + 20b.** No file overlap (`version.rs` vs. worktree/git handling
+ `phase7_cli.rs`), and both are prerequisites for trusting this phase's own
release cut. 20b in particular: while the fixtures stay unreliable, every red CI
run on this phase is ambiguous. Note 20b was **re-sized S → M on 2026-07-22**
after a second, unrelated flake surfaced in the same file (see unit 20b); it now
carries two distinct root causes and a likely product-side component.

**Wave 2 — 20c + 20d.** 20d **blocks on 20a**: its first and most valuable check
is the workspace self-pin invariant, and it must assert against 20a's fix rather
than encode the manual patch as the expected state. 20c is independent but
touches the same CLI dispatch surface, so pairing them in one wave keeps the
conflict surface in a single review.

**Wave 3 — 20e.** Needs a discuss-phase design pass before it is plannable (see
open questions below), and it touches the Ship/outcome path that 20d reasons
about. Sequenced last deliberately: a manual override is most valuable once
reconciliation (18a/18b, shipped in v1.5.0) can already tell the operator *why*
the pipeline is stuck.

## Resolved during discuss-phase (2026-07-22)

- **D-01 — 20e mechanism sharing.** `run_preflight`'s 18f override only works
  because the pipeline process (`devflow start`) is still alive inside
  `run_gate`'s blocking `Gates::poll_response` loop (`pipeline_gate.rs:168`) —
  approving just tells that live loop "don't re-run the check next time
  around." 20e's whole premise is that this process is dead, so a response
  file alone is inert.

  **Decision:** keep one on-disk adjudication record — the same gate-response
  file/schema (`Gates::respond` / `NN-{stage}.response.json`) both paths
  already use. 18f's live poll loop keeps consuming it in-process
  (unchanged). 20e's new `devflow ship --phase N` command is a second,
  out-of-process consumer of the *same* record: it reads the already-written
  Ship response directly and, on `GateAction::Advance`, calls the same
  `finish_workflow()` → `hooks::hooks_after_ship()` batch that `run_gate`
  would have driven (`pipeline_gate.rs:130`) — not a reimplementation of what
  approving Ship means, just a second trigger for the one existing effect.
  — **Reversibility:** costly — inverting this later means untangling a
  second command from a code path (`finish_workflow`) that Phase 16/17 already
  hardened and tested; not undoable as a one-line revert.

- **D-02 — 20e scope of force.** `devflow ship --phase N [--force]` requires
  `state.stage == Stage::Ship` (a Ship gate already written, unconsumed by a
  dead process). It is a recovery for "the approval is stuck," not a shortcut
  past Validate. Any earlier stage returns an error directing the operator to
  resolve that stage first — `--force` never skips Validate. This preserves
  the terminal Ship invariant Phase 16 established and Phase 17 verified
  untouched.

- **D-03 — 20d's ceiling.** `devflow release --check` ships as a read-only
  preflight only in this phase — no executor that runs the actual
  merge/tag/sync/publish sequence. **A backlog item for the future executor
  must be filed** (new `999.N`, mirrored to Linear per this project's backlog
  convention) so the "separate, larger design question" doesn't get lost —
  see Deferred below.

- **D-04 — 20b instance 1 (worktree removal race).** Not locked to a fix
  shape yet. Before planning commits to "retry + `git worktree prune`
  fallback in `cleanup --force`," the phase-researcher must first verify
  whether a real `devflow cleanup --force` run can reach the same
  `Directory not empty` race (not just the `phase7_cli.rs:534` fixture).
  Confirmed-reachable → product fix, test goes green as a consequence.
  Not reachable → fixture-only fix, and say so explicitly rather than
  silently defaulting to a product change.

- **D-05 — 20b instance 2 (object-store corruption).** Also not locked.
  CONTEXT.md's original lean ("no obvious product analog, probably fixture
  durability") stands as the *default*, but the phase-researcher must still
  check whether DevFlow's own git operations (not just the
  `phase7_cli.rs:236` 60-commit-loop fixture) could hit the same
  index/object-store race under real concurrent load before locking this as
  fixture-only. If no product path is found, fix is fsync settings
  (`core.fsyncObjectFiles`/`core.fsync`) on fixture repos and/or shrinking the
  loop's window where it isn't needed for the `>50` threshold.

## Resolved post-research (2026-07-22, after 20-RESEARCH.md)

- **D-06 — 20b instance 1 fix confirmed (resolves D-04).** Research confirmed
  the race is **product-reachable**: `commands::cleanup`
  (`commands.rs:292–335`) removes worktrees unconditionally, never loading
  `workflow::list_states`, never checking `monitor_pid`, never calling the
  existing `liveness()` predicate (`commands.rs:371`, built in 18b for exactly
  this). **Decision:** `cleanup` gains a **hard-refuse** liveness guard — when
  `liveness()` reports `Healthy`/`BetweenStages` for a phase whose worktree it
  is about to remove, refuse and direct the operator to `devflow resume`/wait;
  **no new override flag** (`cleanup --force` already means "also remove the
  reference worktree" per `commands.rs:304` — do not overload it with a second
  meaning). For genuinely-dead phases, add **bounded-backoff retry** on `git
  worktree remove --force`. Do **not** fall back to `git worktree prune` as the
  primary recovery — research confirmed `prune` only removes metadata for
  already-absent directories; it does not delete leftover files, so it would
  orphan files on disk while git believes the worktree is gone.
  — **Reversibility:** reversible — a guard added to one command's path.

- **D-07 — 20c `--until` value range.** Accept the **full `Stage` enum** for
  consistency with how `Stage` is parsed elsewhere, but **explicitly reject
  `--until ship`** with a clear error — research showed it is a semantic no-op
  (`handle_ship_outcome` calls `finish_workflow` directly, never `transition`,
  so the pipeline already stops at Ship). Interception point is
  `pipeline_gate::transition` (`pipeline_gate.rs:51–80`) — the single funnel
  for every stage advance that matters; do **not** intercept
  `loop_back_to_code` (a Validate-failure retry is not "advancing").

- **D-08 — 20b instance 2 `devflow parallel` concern → backlog.** Fix instance
  2 as fixture-durability in this phase (per CONTEXT.md's standing lean).
  Research could not confirm a product analog but flagged `devflow parallel`'s
  concurrent per-worktree commits as *plausible* (no DevFlow-level lock
  serializes them; assumption A1, unconfirmed). **File a new backlog item /
  Linear issue** for that `parallel` object-store concern — same treatment
  D-03 gave the release-cut executor — rather than expanding 20b's scope to
  chase a low-likelihood/high-severity unknown. See Deferred below.

- **D-09 — 20c stop-marker design (researcher recommendation, not user-gated).**
  Follow research Pattern 3 / Assumption A2: a new `State` field (e.g.
  `stopped: bool` + `stop_reason: Option<String>`) with `#[serde(default)]`,
  matching every prior `State` addition (`consecutive_failures`,
  `infra_failures`, `preflight_retries`, `monitor_pid`) — **not** a full
  `workflow::clear_state` (which loses the "stopped at stage X" record
  CONTEXT.md's "persist a terminal-but-not-failed state" phrasing wants).
  **`doctor` reconciliation gap the plan MUST close** (new research finding,
  not in original CONTEXT.md): `check_dead_agent` (`commands.rs:1247–1261`)
  fires `Severity::Problem` for any `is_agent_stage()` phase with a dead agent
  pid — so a `--until plan`-stopped phase (sits at `Stage::Plan`, dead agent
  pid on disk) would be misreported as crashed unless `reconcile_phase` is
  taught to recognize the new stop marker. Without this, the "clean stop
  point" goal is not actually met. — **Reversibility:** costly — a persisted
  `State` field is a serialized on-disk contract; removing it later needs a
  `#[serde(default)]`-compatible migration, same as any prior field.

</decisions>

<units>
## 20a — `VersionBump` must rewrite workspace member self-pins

**Source:** v1.6.0 release (2026-07-22); the identical defect was previously
patched by `7ad260c` for v1.5.0.

`version::write_version` rewrites exactly one dotted field — `field_for()`
returns `"workspace.package.version"` for a workspace `Cargo.toml`, and
`replace_version_in_contents` rewrites that field alone. But a published Cargo
workspace states its version twice:

```toml
[workspace.package]
version = "1.6.0"                                                  # VersionBump writes this

[workspace.dependencies]
devflow-core = { path = "crates/devflow-core", version = "1.6.0" } # nothing writes this
```

The self-pin cannot use `version.workspace = true` — Cargo has no interpolation
for dependency versions — and cannot be omitted, because a path dependency of a
*published* crate requires an explicit version.

**Why fix rather than document.** It has shipped broken two for two (v1.5.0
patched by `7ad260c`, v1.6.0 by release-prep PR #15). The failure mode is
invisible until the last step of a release: everything builds, every test
passes, clippy is clean — a `path` dependency resolves locally and ignores the
`version` field entirely. It detonates at `cargo publish`, where the registry
rejects the upload as a duplicate. On release day. After `main` is tagged.

This is a **product** bug, not a repo chore: any DevFlow user with a published
Cargo workspace hits it identically and gets the same opaque duplicate-version
error with no hint about the cause.

**Proposed fix.** After writing `workspace.package.version`, also rewrite every
`[workspace.dependencies]` entry whose `path` points at a workspace member. That
is a general rule, not a special case: *a dependency on a crate in this workspace
carries this workspace's version.*

Care needed:
- Only entries with a local `path`. A third-party dep like
  `serde = { version = "1" }` has a version but no path and must not be touched.
- Extend the existing hand-rolled TOML handling rather than pulling in a parser
  dependency — `version.rs` is deliberately hand-rolled.

**Already landed — a guard, not the fix.**
`crates/devflow-cli/tests/workspace_version_pin.rs` (PR #17) asserts every
workspace-member pin equals `[workspace.package] version`, RED-proven against
the real defect. It converts a silent release-day rejection into a loud
pre-merge failure, but the manual bump is still required every release until
`VersionBump` handles it.

**Rejected alternatives.**
- *Loosen the pin to `version = "1"`* — the symptom vanishes across 1.x, but
  `devflow 1.6.0` could then resolve against `devflow-core 1.5.0`. These crates
  release in lockstep and the CLI is tightly coupled to core, so that skew would
  surface as baffling runtime behavior. It also just defers the problem to 2.0.
- *Adopt `cargo-release` / `cargo-workspaces`* — solves it, but partly
  duplicates the tool whose entire purpose is automating releases.

## 20b — `phase7_cli.rs` git fixtures are unreliable under CI

**Source:** two GitHub Actions flakes on 2026-07-22, on different tests with
different symptoms. Originally filed as one flaky test (999.23); **broadened
after the second instance** — the common factor is the file's git-fixture
approach, not any one test. DEN-48 carries the current framing; the repo's
999.23 CONTEXT.md only ever captured instance 1.

**Instance 1 — worktree removal race.**
`reference_and_cleanup_worktree_cli_flow` (`phase7_cli.rs:534`), release PR #13,
run `29939619958`:

```
devflow ["cleanup", "--force"] failed
error: git worktree command failed: error: failed to delete '.git/worktrees/phase-08': Directory not empty
```

**Instance 2 — git object store corruption.**
`start_worktree_mode_ignores_main_checkout_divergence` (`phase7_cli.rs:236`),
guard PR #17, run `29946629986`:

```
git ["commit", "-q", "-m", "commit 47"] failed
stderr: error: invalid object 100644 abc4eff6ac83026669840d289fce80cc9a42baaa for 'f46.txt'
```

The index referenced an object absent from the object store, mid-way through the
60-commit loop at `:246` (which exists only to cross the `behind > 50`
hard-fail threshold), in an isolated `tempfile::tempdir()` repo.

**Evidence both are flakes, not regressions.** For each: a sibling CI job on
byte-identical code passed in the same workflow run; the tests are untouched by
the changes under review; they pass repeatedly locally (instance 1: 5/5;
instance 2: 3/3 for the file plus a full 439-test workspace run); a bare re-run
with no code change went green. Instance 1 predates the guard PR entirely, which
rules out "adding a test binary widened a race by increasing parallelism."

**Why it matters.** Both landed on **release-path PRs**. A test that fails on a
coin flip there makes release-day CI unreliable and trains the reader to re-run
red CI instead of reading it — the exact reflex that eventually lets a real
regression through. Fifth and sixth instance of the broader family in this
project's history (WR-03 / 18-02 parallel-worktree capture timing; 17-09 GAP-2
concurrent-ship gate wedge; 19i PATH race), so treat it as a structural weakness
in how these fixtures drive real `git` under CI concurrency — not two unlucky
tests.

**Likely causes.**
- *Instance 1:* `git worktree remove` racing the filesystem — a handle still
  open inside `.git/worktrees/phase-08` between the removal attempt and the
  directory unlink.
- *Instance 2:* a loose object write not yet visible to the index read that
  follows it. Consistent with `/tmp` filesystem behavior on shared runners under
  concurrent test-binary load, where fsync ordering is weaker than a local dev
  machine's.

Both are widened by CI concurrency and near-unreproducible locally — which is
exactly why they need a structural fix rather than a retry loop bolted on after
the next red build.

**Possible shapes (not yet decided).**
- Make `cleanup --force` tolerate `Directory not empty` with bounded-backoff
  retry, then fall back to `git worktree prune`. Likely a **product** fix — a
  real user running `devflow cleanup --force` hits the same opaque error.
- Give the fixtures stronger durability settings for test use (explicit
  `core.fsyncObjectFiles` / `core.fsync` on fixture repos) so an object write is
  visible to the following index read.
- Reduce fixture cost where the test does not need it — the 60-commit loop is a
  wide window for exactly this failure.
- Consider serializing the git-heavy tests in this file if isolation alone
  cannot make them robust.

Determine whether each is reachable by a real user before fixing it test-side
only.

## 20c — Plan-only pipeline mode (`--until <stage>`)

**Source:** dogfood attempt 2026-07-20 — tried to run GSD planning for Phase 18
through devflow and found no way to stop after Plan.

`devflow start` always runs the full Define → Plan → Code → Validate → Ship
pipeline. `--mode supervise` only changes *where it gates* (Validate and Ship) —
the Code stage still runs unattended. There is no `--until` flag and no config
knob.

Consequence: "use devflow to just do the planning" is not expressible. The only
way to stop after Plan is to kill the monitor mid-pipeline, which strands phase
state and orphans a worktree — precisely the mess 18a (doctor reconciliation)
and 18b (monitor liveness) exist to clean up.

**Proposed shape.** `devflow start --until <stage>` halts cleanly after the named
stage completes: persist a terminal-but-not-failed state, emit a
`workflow_finished` event with an explicit "stopped at requested stage" reason,
and leave no polling monitor behind. `--until plan` is the motivating case
(produce PLAN.md files, then hand back to a human), but `--until code` and
`--until validate` are equally reasonable.

**Why it matters beyond convenience.** Dogfooding is this project's
highest-yield bug source, and the cheapest dogfood run is the one that exercises
the fewest stages. Without a clean stop point, every dogfood run is
all-or-nothing: either run the full pipeline (which merges, tags, and releases)
or don't dogfood at all. That directly discourages the small, frequent runs that
surface the most findings.

## 20d — Release-cut automation (`devflow release --check`)

**Source:** v1.5.0 release session, 2026-07-21 — three distinct manual-process
failures in one release cut.

DevFlow automates the *phase* pipeline thoroughly, but the *release-cut* step —
version-bump PR → merge to `main` → tag → sync `develop` → publish to crates.io
— is a fully manual, hand-run checklist (`CONTRIBUTING.md` § "Cutting a
Release"). Cutting v1.5.0 hit four failures from this gap:

1. **`devflow-core` version pin drift.** Second occurrence — PR #10 (`c9aff7f`)
   fixed the identical drift once already, by hand. Partially addressed by
   moving `devflow-core` into `[workspace.dependencies]`, but there is still no
   enforcement that the pinned version and the workspace version cannot diverge
   undetected. **Superseded in part by 20a**, which makes `VersionBump` write
   the pin; 20d's check becomes the belt-and-braces assertion on top.
2. **`main`/`develop` divergence.** Because `main` only accepts squash merges,
   `develop` silently fell behind by a full release cycle before this was
   caught, producing 11 file conflicts on the next release PR. Fixed with
   `scripts/sync-main-to-develop.sh`, but running it is an unenforced manual
   step — nothing stops the next release from skipping it again.
3. **crates.io publish ordering was undocumented.** `cargo publish --dry-run -p
   devflow` fails to compile until `devflow-core` is live on the registry at a
   satisfying version, since dry-run/verify resolves the path dependency against
   the *published* registry version, not local source. Discovered by trial and
   error; now documented in `CONTRIBUTING.md`, but only as prose.
4. **Tag-signing had no preflight.** The official signed tag failed repeatedly
   with opaque `ssh_askpass`/agent errors before the environment issue was
   diagnosed. DevFlow's own automated version-bump tags already scope off
   signing entirely (`git.rs::tag()` forces `tag.gpgsign=false` per-invocation,
   confirmed at HEAD) — this is specifically about the *manual*, human-run
   official release tag, which has no equivalent safety net.

**Proposed shape.** `devflow release --check` (name TBD) as a preflight command,
run before attempting the actual tag:

- Verify every workspace-member self-pin matches `[workspace.package].version`
  (asserting 20a's invariant).
- Verify `develop`'s tip is reachable as an ancestor check against `origin/main`
  (i.e. `scripts/sync-main-to-develop.sh` would be a no-op) before a new release
  PR can be described as ready.
- State the crates.io publish order as a structured check, not just prose.
- Check tag-signing viability: if `tag.gpgsign` is `true`, confirm a signing key
  is actually reachable (`ssh-add -l` / `gpg-connect-agent` succeeds) *before*
  attempting `git tag -s`, with an actionable error instead of the opaque
  `ssh_askpass: exec(...): No such file or directory` failure.

Distinct from 20e, which is about recovering a stuck *phase* mid-pipeline. This
is the top-level version-cut process that happens outside any single phase's
Ship stage.

## 20e — Manual ship override

**Source:** operator request 2026-07-20, during the Phase 18 planning dogfood
attempt.

A command that lets an operator drive a phase through Ship by hand, without
depending on a live monitor to consume a gate response.

**Why `devflow gate approve` does not already cover this** (verified at source
2026-07-20):

1. **`respond()` refuses when no gate is open** — `gates.rs:186` returns
   `GateError::NoOpenGate` (test: `respond_refuses_when_no_gate_is_open`). If
   the monitor died before it ever wrote the Ship gate request, there is nothing
   to approve and no way in.
2. **Approving only writes a response file.** `respond()` writes
   `NN-ship.response.json`; a *live monitor polling that path* is what actually
   advances the workflow. If the monitor is dead, the approval sits unconsumed
   forever and nothing happens.

So the existing gate commands assume a healthy pipeline. The gap is recovery
when that assumption fails, which on this project's dogfood history is not rare.

**Proposed shape.** `devflow ship --phase N [--force]` (name TBD) that executes
the terminal transition directly: run the after-ship hook batch (Merge,
VersionBump, ChangelogAppend, BranchCleanup) in-process, honoring the existing
fail-closed contract — a failed Merge must still stop the batch, preserve state,
and refuse to emit `workflow_finished` (the Phase 16 invariant,
regression-tested).

Note the interaction with 20a: a manual ship path runs `VersionBump`, so it
inherits the self-pin fix for free once 20a lands — another reason 20a sequences
first.

</units>

<verification>
## Source claims re-verified at HEAD (2026-07-22, during promotion)

All five items were re-checked against `develop` at `8ecbdf9`. None are stale:

| Unit | Check | Result |
|---|---|---|
| 20a | `version.rs:198–205` — `write_version` resolves one field via `field_for()`, no `[workspace.dependencies]` pass | **Open** |
| 20b | `phase7_cli.rs:534` and `:236` — both flaking tests present and unmodified; the 60-commit loop at `:246` confirmed | **Open** |
| 20c | `rg '--until|stop_after|plan_only|StopAfter' crates/` → zero hits | **Open** |
| 20d | no `release` subcommand in `commands.rs` | **Open** |
| 20e | `GateCmd` exposes `Approve` only; no force-ship path | **Open** |

Also confirmed: PR #17's `workspace_version_pin.rs` guard is present, and it is
a guard only — `write_version` is unchanged by it.

</verification>

<canonical_refs>
## Canonical References

**Downstream agents (researcher, planner) MUST read these before planning or
implementing 20e in particular — they ground D-01/D-02 above.**

### 20e — gate/ship mechanism
- `crates/devflow-core/src/gates.rs` — `respond()` (`:179`), `NoOpenGate`
  (`:93`); the on-disk adjudication record 20e must reuse, not reinvent.
- `crates/devflow-cli/src/pipeline_gate.rs` — `run_gate()` (`:168`, the live
  blocking poll 18f depends on and 20e cannot depend on),
  `finish_workflow()` (`:130`, the after-ship hook batch 20e must call
  directly).
- `crates/devflow-cli/src/preflight.rs` — 18f's `GateAction::Advance`
  override (`:188`, `:243`) — the sibling mechanism 20e's design note (D-01)
  explicitly builds on rather than duplicates.
- `crates/devflow-cli/src/pipeline_outcomes.rs` — `handle_ship_outcome()`
  (`:275`), showing the Ship gate is always-gated regardless of mode; 20e
  must reach the same terminal state through its second entry point.

### 20a — version self-pin
- `crates/devflow-core/src/version.rs` — `write_version` / `field_for()`
  (lines ~198–205 per the verification table above). **Path corrected
  2026-07-22 during research:** discuss-phase wrote `devflow-cli` here; the
  file lives in `devflow-core`. Verified with `find crates -name version.rs`.
- `crates/devflow-cli/tests/workspace_version_pin.rs` (PR #17) — the existing
  RED-proven guard; 20a's fix must turn this from a guard into a no-op-by-
  construction, not replace it.

### 20b — flaky fixtures
- `crates/devflow-cli/tests/phase7_cli.rs:534` — instance 1
  (`reference_and_cleanup_worktree_cli_flow`).
- `crates/devflow-cli/tests/phase7_cli.rs:236` (60-commit loop at `:246`) —
  instance 2 (`start_worktree_mode_ignores_main_checkout_divergence`).

</canonical_refs>

<deferred>
## Deferred

**999.3 — CLI Operator Discoverability** (Low/L, DEN-28) stays in the backlog.
The Phase 19 scoping note reserved it for Phase 20 "likely", but it is the only
Low-priority item in that set and it bundles four distinct gaps (`gate show`,
rate-limit reset surfacing, in-stage `status` progress, recovery-verb
discoverability). Split it into smaller issues before promoting; it should not
ride along as the largest, lowest-value unit in a phase that already carries two
L-sized items.

**A `devflow release` that executes** (rather than just `--check`) — locked as
out of scope per D-03. **Action required before/at ship:** file a new backlog
item (next `999.N`, mirrored to Linear per `reference-linear-devflow-project`
convention) for "release-cut executor: merge PR → tag → sync develop → publish"
so this doesn't silently disappear once 20d's `--check` ships. Not filed yet —
this phase's `git_commit`/`update_state` steps do not create Linear issues;
raise it explicitly during `/gsd-review-backlog` or at Phase 20 ship time.

**`devflow parallel` object-store race** (per D-08) — file a new backlog item /
Linear issue for "confirm-or-refute whether `devflow parallel`'s concurrent
per-worktree commits can hit the same git object-store corruption as 20b
instance 2 (no DevFlow-level lock serializes them; research assumption A1,
unconfirmed)." Low likelihood, high severity if real. Out of scope for Phase 20
— instance 2 is fixed fixture-side here; this is the separate product-side
follow-up. Not filed yet — same caveat as the release-cut executor above.

</deferred>
