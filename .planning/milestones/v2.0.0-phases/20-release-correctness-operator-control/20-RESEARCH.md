# Phase 20: Release Correctness + Operator Control - Research

**Researched:** 2026-07-22
**Domain:** Internal Rust CLI/state-machine correctness (no new external
libraries) — release tooling (`version.rs`, `hooks.rs`, git.rs), CI test
reliability (git-fixture concurrency), and pipeline operator controls
(gate/state machine in `pipeline_gate.rs`/`pipeline_launch.rs`).
**Confidence:** HIGH — every claim below is source-verified against `develop`
at the researched HEAD (`46a5f7b`) by reading the actual implementation, not
inferred from CONTEXT.md's descriptions. Two claims in CONTEXT.md's own
canonical-refs table were found stale during this pass (see Assumptions Log
and Common Pitfalls).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Sequencing.** Wave 1 — 20a + 20b (no file overlap, both gate this phase's
own release cut). Wave 2 — 20c + 20d (20d **blocks on 20a**: its first check
asserts the workspace self-pin invariant and must not encode today's manual
patch as the expected state). Wave 3 — 20e (blocked on a discuss-phase design
pass; touches the Ship/outcome path 20d reasons about).

**D-01 — 20e mechanism sharing.** Keep one on-disk adjudication record (the
same `Gates::respond`/`NN-{stage}.response.json` schema 18f's live poll loop
already uses). 20e's new `devflow ship --phase N` command is a second,
out-of-process consumer of the *same* record: it reads the already-written
Ship response directly and, on `GateAction::Advance`, calls the same
`finish_workflow()` → `hooks::hooks_after_ship()` batch `run_gate` would have
driven — not a reimplementation of what approving Ship means, just a second
trigger for the one existing effect. Reversibility: costly.

**D-02 — 20e scope of force.** `devflow ship --phase N [--force]` requires
`state.stage == Stage::Ship` (a Ship gate already written, unconsumed by a
dead process). It is a recovery for "the approval is stuck," not a shortcut
past Validate. Any earlier stage returns an error directing the operator to
resolve that stage first — `--force` never skips Validate.

**D-03 — 20d's ceiling.** `devflow release --check` ships as a read-only
preflight only in this phase — no executor that runs the actual
merge/tag/sync/publish sequence. A backlog item for the future executor must
be filed (new `999.N`, mirrored to Linear) so that larger design question
isn't lost.

**D-04 — 20b instance 1 (worktree removal race).** Not locked to a fix shape
yet. Before planning commits to "retry + `git worktree prune` fallback in
`cleanup --force`," the phase-researcher must first verify whether a real
`devflow cleanup --force` run can reach the same `Directory not empty` race
(not just the `phase7_cli.rs:534` fixture). **Resolved by this research:
confirmed-reachable — `cleanup` has zero liveness check (see Architecture
Patterns, Pattern 2) — this is a product fix, and the test goes green as a
consequence.**

**D-05 — 20b instance 2 (object-store corruption).** Also not locked.
CONTEXT.md's original lean ("no obvious product analog, probably fixture
durability") stands as the default, but the phase-researcher must check
whether DevFlow's own git operations (not just the `phase7_cli.rs:236`
60-commit-loop fixture) could hit the same race under real concurrent load.
**Addressed by this research: the default (fixture-only) lean stands as the
phase's scope, but a plausible-not-confirmed `devflow parallel`
shared-object-store analog is recorded as an open item (see Common Pitfalls
#4, Assumption A1) rather than silently closed.**

### Claude's Discretion

- Exact shape of 20a's TOML-rewrite extension (which lines/helpers in
  `version.rs` to extend), so long as the hand-rolled approach and GAP-6
  comment/quote preservation guarantees are kept.
- Exact shape of 20b's fixture stabilization (retry/backoff parameters,
  whether to serialize the git-heavy tests in the file) and of the `cleanup`
  liveness-guard's UX (hard refuse vs. warn — flagged as Open Question 2
  below, not resolved by this research).
- Exact `State` representation for 20c's stop marker (new field vs. other
  shape — flagged as Assumption A2, a plan/discuss-phase decision).
- Exact command name/flag spelling for `devflow release --check` (20d) and
  `devflow ship` (20e) — CONTEXT.md marks both "name TBD."
- Exact set of `--until` targets to accept (this research recommends
  accepting the full `Stage` enum for parser consistency; see Open
  Question 1).

### Deferred Ideas (OUT OF SCOPE)

- **999.3 — CLI Operator Discoverability** (Low/L, DEN-28) stays in the
  backlog — deliberately left behind, not part of this phase. It bundles
  four distinct gaps (`gate show`, rate-limit reset surfacing, in-stage
  `status` progress, recovery-verb discoverability) that deserve splitting
  before promotion.
- **A `devflow release` that executes** (rather than just `--check`) —
  locked out of scope per D-03. A new backlog item for "release-cut
  executor: merge PR → tag → sync develop → publish" must be filed
  (mirrored to Linear) before/at this phase's ship time — not filed yet.
</user_constraints>

<phase_requirements>
## Phase Requirements

This project uses no formal `REQ-ID` scheme (confirmed: no
`.planning/REQUIREMENTS.md` exists in this repository). Phase 20's units are
identified by letter (20a–20e) per CONTEXT.md, not numbered requirements.

| ID | Description | Research Support |
|----|-------------|-------------------|
| 20a | `VersionBump` must rewrite workspace member self-pins in `[workspace.dependencies]`, not just `[workspace.package] version` | Architecture Patterns Pattern 1; exact function/line citations for `write_version`/`field_for`/`replace_version_in_contents`; existing `workspace_version_pin.rs` guard confirmed present and must stay passing without further manual edits. Corrects CONTEXT.md's stale file-path citation (see Common Pitfalls #1). |
| 20b | `phase7_cli.rs` git fixtures unreliable under CI — worktree-removal race (instance 1) and object-store corruption (instance 2) | Architecture Patterns Pattern 2 (instance 1 confirmed product-reachable via `cleanup`'s missing liveness check — new finding) and Common Pitfalls #3/#4 (prune's real scope; instance 2's plausible-not-confirmed `devflow parallel` analog). Resolves D-04 conclusively; addresses D-05 without overclaiming. |
| 20c | `devflow start --until <stage>` halts cleanly, no orphaned monitor or worktree | Architecture Patterns Pattern 3 — exact interception point (`pipeline_gate::transition`) and a previously-undocumented doctor false-positive gap (`check_dead_agent`) that must be closed for the feature to actually be "clean." |
| 20d | `devflow release --check` preflight — self-pin, `develop`/`main` divergence, publish order, tag-signing viability | Architecture Patterns Pattern 4 — live-verified `gpg.format=ssh` finding and the exact `ssh-add -l` exit-code contract; Don't Hand-Roll table for the ancestor-check command already proven in `scripts/sync-main-to-develop.sh`. |
| 20e | Manual ship override honoring the fail-closed terminal Ship invariant | Architecture Patterns Pattern 5 — independently re-derives CONTEXT.md's D-01/D-02 design from source and confirms it fully correct with no corrections needed. |
</phase_requirements>

## Summary

This phase has no new-library research surface — it is five internal
correctness/control fixes against a codebase this project already owns. The
highest-value research finding is that **three of the five units have a
sharper root cause or a sharper fix shape than CONTEXT.md described**, found
by reading the actual source rather than trusting the write-up:

1. **20a's canonical-refs path is wrong.** `version.rs` lives in
   `crates/devflow-core/src/version.rs`, not `crates/devflow-cli/src/version.rs`
   as CONTEXT.md's `<canonical_refs>` states. The fix itself is small and
   low-risk: `write_version`'s existing single-field rewrite and
   `hooks::version_bump`'s existing single `commit_path` call already cover a
   second field in the *same* file — no new commit call is needed.
2. **20b instance 1 (worktree removal race) is CONFIRMED product-reachable,
   with a concrete, previously-undocumented mechanism**: `commands::cleanup`
   calls `worktree::remove(.., force)` for every worktree with **zero
   liveness check** — it never consults `workflow::list_states` or the
   `monitor_pid`/`liveness()` machinery 18a/18b already built. A real user
   running `devflow cleanup --force` while a phase's monitor is still alive
   (mid-pipeline, Auto mode) races the still-running agent process's writes
   inside the worktree against `git worktree remove`'s clean-directory scan —
   this is the exact "Directory not empty" failure mode, and it needs no
   fixture to reach it. This is new information beyond what D-04 asked the
   researcher to check.
3. **20b's proposed fallback (`git worktree prune`) is insufficient by
   itself.** Per the official git docs, `prune` only cleans up
   `$GIT_DIR/worktrees/<name>` administrative metadata for a working
   directory that is **already gone from disk** — it does not delete
   leftover files. If `git worktree remove --force` fails with `Directory
   not empty`, the correct recovery is retry-with-backoff (the race is
   transient) and/or an explicit `fs::remove_dir_all` before `prune`, not
   `prune` alone.
4. **20c's clean interception point is `pipeline_gate::transition()`, not a
   new command.** All three meaningful `--until` targets (plan, code,
   validate) advance via this one function; Ship never calls it (it's
   already terminal). But stopping there is not sufficient on its own:
   `devflow doctor`'s `check_dead_agent` (a `Severity::Problem`, not `Warn`)
   will misdiagnose an intentionally-stopped agent-stage phase as stuck
   unless the stop path also clears/marks state so `agent_pid`/`monitor_pid`
   reconciliation doesn't fire. This is a real gap in the existing 18a/18b
   reconciliation checks that CONTEXT.md's proposed shape does not mention.
5. **20d's tag-signing check must branch on `git config gpg.format`.** This
   repo's actual git config (verified live, not assumed) uses
   `gpg.format=ssh` with `user.signingkey` pointing at an SSH public key —
   not classic GPG. The check CONTEXT.md describes
   (`gpg-connect-agent`/`ssh-add -l`) needs to pick the right one per
   `gpg.format`, and for the `ssh` case, `ssh-add -l`'s three distinct exit
   states (2 = no agent, 1 = agent reachable but empty, 0 = keys listed) map
   directly to three distinct, actionable error messages.

**Primary recommendation:** Treat 20a/20b/20c as source-grounded fix/design
work with the specifics above; treat 20d's signing check as `gpg.format`-aware
from the start (not a GPG-only check with SSH bolted on later); treat 20e per
CONTEXT.md's D-01/D-02 exactly as written — that design is already correct
against source and needs no correction here.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Version-file rewrite (20a) | Core library (`devflow-core::version`) | — | Pure file-mutation logic, no CLI/state coupling; already lives in `devflow-core`. |
| Git-fixture reliability (20b) | Test infrastructure | CLI (`commands::cleanup`) | Fixture fix is test-tier; the worktree-removal race is a real CLI-command defect (`cleanup`) that also needs a product-tier fix. |
| Plan-only pipeline mode (20c) | CLI pipeline state machine (`pipeline_gate::transition`) | Core (`State`/`Stage`) | The stop point is a control-flow decision inside the existing stage-transition funnel, not a new subsystem. |
| Release-cut preflight (20d) | CLI (new `devflow release --check` command) | Core (`version`, `git`) | A read-only diagnostic command; reuses core version/git primitives, adds no new core capability. |
| Manual ship override (20e) | CLI (new `devflow ship` command) | Core (`gates::Gates`, `hooks::hooks_after_ship`) | A second, out-of-process consumer of the existing on-disk gate-response record; the effect it triggers (`finish_workflow`) is unchanged. |

## Package Legitimacy Audit

**Not applicable — this phase introduces zero new external dependencies.**
All five units extend existing internal modules (`devflow-core::version`,
`devflow-core::git`, `devflow-cli::commands`, `devflow-cli::pipeline_gate`,
`devflow-cli::pipeline_launch`) using crates already present in
`Cargo.lock`. Specifically verified during this research:

- `devflow-core` already depends on `toml = "1.1.2+spec-1.1.0"` (used today
  in `config.rs` and `doc_check.rs`, which even parses the root `Cargo.toml`
  already). **20a does not need a new dependency to use a TOML parser** —
  one is already in the dependency graph. The reason to keep hand-rolling
  `version.rs`'s edit (per CONTEXT.md) is real but is NOT "no parser
  available": it is that the plain `toml` crate's `to_string`/serialize does
  not preserve comments, quote style, or key ordering on round-trip (no
  `toml_edit` — the format-preserving layer — appears anywhere in
  `Cargo.lock`), and `version.rs` has existing regression tests (GAP-6:
  `write_version_preserves_trailing_comment_in_toml`,
  `_in_single_quoted_toml`, `_trailing_comma_in_package_json`) that a
  parse-and-reserialize round trip would break. State this reason correctly
  in the plan rather than repeating CONTEXT.md's "no parser dependency"
  framing, which is technically inaccurate.
- No new crate is needed for 20c (`--until <stage>` reuses the existing
  `Stage` clap-parseable enum), 20d (shells out to `git`/`ssh-add`/
  `gpg-connect-agent`, all already-external-process patterns this codebase
  uses throughout `git.rs`/`worktree.rs`), or 20e (reuses `gates::Gates` and
  `hooks::hooks_after_ship`).

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.

## Architecture Patterns

### System Architecture Diagram — pipeline stop/recovery seams this phase touches

```
 devflow start --until <stage>          devflow ship --phase N [--force]
        │                                        │
        ▼                                        ▼
   State::new(Define) ──► launch_stage      load persisted State
        │                  (spawns monitor)      │ (must be Stage::Ship,
        │                                         │  D-02: no earlier stage)
        ▼                                        ▼
  [agent runs, exits]                    Gates::response_path exists?
        │                                        │
        ▼                                   ┌────┴────┐
  devflow advance (monitor-invoked)         yes        no → error, direct
        │                                    │           to resolve stage
        ▼                                    ▼
  outcome_policy::decide_action        read GateResponse,
        │ Action::Advance                GateAction::from_response
        ▼                                    │
  pipeline_gate::transition(from, to)   GateAction::Advance?
        │                                    │
   ┌────┴─────────────────────┐              ▼
   │ from == state.stop_until?│         finish_workflow(project_root, state)
   │  (NEW in 20c)             │              │  (SAME function run_gate's
   └────┬──────────────┬───────┘              │   live poll loop would have
       yes             no                     │   called — not reimplemented)
        │               │                     ▼
        ▼               ▼                run_checkout_hooks(hooks_after_ship)
  emit workflow_finished   launch_stage(to)    │  Merge → VersionBump →
  (reason: stopped),        (existing path)    │  ChangelogAppend → BranchCleanup
  clear monitor_pid,                           ▼
  leave state recoverable                 workflow::clear_state + emit
  (NOT Severity::Problem                  workflow_finished
   to doctor — 18a/18b gap
   this phase must also close)
```

### Recommended structure — no new files required

All five units extend existing modules in place:

```
crates/devflow-core/src/
├── version.rs        # 20a: extend write_version's field rewrite
├── git.rs             # 20d: add ancestor-check / publish-order / signing helpers (or a new small module)
crates/devflow-cli/src/
├── commands.rs        # 20b: cleanup() liveness guard; 20d: `release --check` handler
├── pipeline_gate.rs   # 20c: transition() interception; 20e: new ship() handler (or commands.rs)
├── pipeline_launch.rs # 20c: launch_stage's stop-aware caller, if the check needs to live here instead
├── main.rs            # 20c: `--until` flag on Start; 20e: new `Ship` subcommand
crates/devflow-cli/tests/
├── phase7_cli.rs      # 20b: fixture stabilization for both instances
├── workspace_version_pin.rs  # 20a: existing guard stays; write_version fix makes it pass without manual edits
├── help_snapshot.rs   # 20c/20e: MUST be regenerated — any new flag/subcommand changes `devflow --help`
```

### Pattern 1: Single-field TOML rewrite, extended to N fields (20a)

**What:** `version.rs`'s `write_version` currently rewrites exactly one
`field_for()`-resolved dotted path per call. The fix generalizes this to
rewrite the `[workspace.package] version` field AND every
`[workspace.dependencies].<crate>.version` sub-value where that dependency
table also has a local `path = "crates/..."` key — using the same
line-scanning approach `replace_version_in_contents` already uses, extended
to detect inline-table dependency entries (`name = { path = "...", version =
"..." }` on one line) rather than the section-header-scoped `key = value`
form it handles today.

**When to use:** Only for workspace `Cargo.toml` (the `field_for` branch that
already special-cases `[workspace.package]`). Plain-package `Cargo.toml`,
`pyproject.toml`, `package.json` are unaffected — there's no analogous
self-pin in those formats for this project.

**Example (current single-field code, to be extended):**
```rust
// Source: crates/devflow-core/src/version.rs:196-206 (verified at HEAD)
pub fn write_version(project_root: &Path, version: &Version) -> Result<PathBuf, VersionError> {
    let path = detect_version_file(project_root)
        .ok_or_else(|| VersionError::Parse("no version file found".into()))?;
    let contents = std::fs::read_to_string(&path)?;
    let field = field_for(&path, &contents);
    let replaced = replace_version_in_contents(&contents, field, &version.to_string())
        .ok_or_else(|| VersionError::Parse(format!("field `{field}` not found")))?;
    std::fs::write(&path, replaced)?;
    Ok(path)
}
```
The fix must NOT touch `hooks::version_bump` (`crates/devflow-core/src/hooks.rs:230-258`)
— it calls `write_version` once and commits `path.file_name()` (the same
`Cargo.toml`), so both fields land in the same commit for free once
`write_version` itself rewrites both.

**Existing guard the fix must satisfy, unmodified:**
`crates/devflow-cli/tests/workspace_version_pin.rs` (PR #17) — a RED-proven
regression test asserting every `[workspace.dependencies]` path-dependency
pin equals `[workspace.package] version`. 20a's fix should make this pass
without a manual edit ever again; do not weaken or delete the guard.

### Pattern 2: Liveness-gated worktree removal (20b instance 1 — product fix)

**What:** `commands::cleanup` (`crates/devflow-cli/src/commands.rs:292-335`)
iterates every worktree under `.worktrees/` and calls
`worktree::remove(project_root, &wt.path, force)` unconditionally — it never
loads `workflow::list_states`, never checks `monitor_pid`, and never calls
the existing `liveness()` predicate (`commands.rs:371`, built for exactly
this purpose in 18b). A real `devflow start --phase N` in Auto mode leaves a
detached monitor alive through Code/Validate/Ship; if a user runs `devflow
cleanup --force` before that monitor reaches a terminal state, the
still-running agent process (cwd inside the worktree) can write new files
into the directory in the exact window `git worktree remove` is scanning it
for cleanliness, producing `Directory not empty`.

**When to use:** `cleanup` should check `liveness(state.monitor_pid, ...)`
for any phase whose worktree it is about to remove and refuse (or require an
explicit stronger flag) when the phase is `Healthy`/`BetweenStages` — mirror
the existing `Liveness` enum instead of introducing a new one.

**Retry/fallback shape, corrected against git's own documentation:**
`git worktree prune` does **not** delete leftover files — it only removes
`$GIT_DIR/worktrees/<name>` metadata for a working directory *already
absent* from disk. A bounded retry (a few short backoff attempts) of `git
worktree remove --force` is the correct primary recovery for a transient
race; falling straight to `prune` without ensuring the directory is actually
gone would leave orphaned files on disk while git's bookkeeping believes the
worktree no longer exists.

### Pattern 3: Central stop-point interception for `--until` (20c)

**What:** Every stage advance that matters for `--until` (Define→Plan,
Plan→Code, Code→Validate, and Validate's internal advance to Ship) funnels
through exactly one function: `pipeline_gate::transition` (`pipeline_gate.rs:51-80`).
`handle_validate_outcome` (`pipeline_outcomes.rs:213-272`) calls `transition(..,
Stage::Ship)` from three different branches (ambiguous-gate-advance,
mode-gated-advance, ungated-pass) — all three funnel through the same
`transition` call, so intercepting there (rather than in each of the three
call sites) covers Validate correctly too. Ship itself never calls
`transition` — `handle_ship_outcome` calls `finish_workflow` directly, so
`--until ship` is a semantic no-op (the full pipeline already stops there
today); the CLI should reject or no-op that combination explicitly rather
than silently accepting it.

**Where NOT to intercept:** `loop_back_to_code` (`pipeline_gate.rs:84-92`)
calls `launch_stage` directly, bypassing `transition` — this is correct and
must stay untouched; a loop-back is not "advancing," so `--until` must never
interrupt a Validate-failure retry back to Code.

**The doctor/reconciliation gap this phase must close (new finding, not in
CONTEXT.md):** `check_dead_agent` (`commands.rs:1247-1261`) fires
`Severity::Problem` whenever `facts.stage.is_agent_stage()` is true and the
recorded `agent_pid` is not alive — Define/Plan/Code are all `is_agent_stage()
== true`. A phase stopped by `--until plan` sits at `Stage::Plan` with a
now-dead agent pid on disk. Unless the stop path explicitly marks this state
as intentionally terminal (and `reconcile_phase`'s checks are taught to
recognize that marker), every `--until`-stopped phase will show up in
`devflow doctor` as a `Problem` indistinguishable from a genuinely crashed
agent, defeating the "clean stop point" goal. The plan must address this —
either a new explicit state field (e.g. `stopped: bool` /
`stop_reason: Option<String>`, following the exact `#[serde(default)]`
backward-compat pattern every other `State` field added since 17-01 uses:
`consecutive_failures`, `infra_failures`, `preflight_retries`, `monitor_pid`)
or full `workflow::clear_state` (losing the record) — CONTEXT.md's "persist
a terminal-but-not-failed state" phrasing implies the former, not the
latter.

### Pattern 4: `gpg.format`-aware signing-viability check (20d)

**What:** `git config --get gpg.format` on THIS repository (verified live,
not from training data) returns `ssh`, with `user.signingkey` pointing at an
SSH public key file (`~/.ssh/github_ed25519.pub` on the researched machine —
path itself is host-specific and must not be hardcoded in the check). The
documented failure in CONTEXT.md (`ssh_askpass: exec(...): No such file or
directory`) is the exact symptom of git falling back to `SSH_ASKPASS` when
the ssh-agent doesn't have the signing key loaded under `gpg.format=ssh`.

**Check shape:**
```
gpg.format unset or "openpgp" → verify a secret key exists for
  user.signingkey (e.g. `gpg --list-secret-keys <keyid>` succeeds)

gpg.format == "ssh" → verify:
  1. user.signingkey is set and the file exists
  2. `ssh-add -l` exit code:
       2 → "no ssh-agent reachable" (SSH_AUTH_SOCK unset/dead)
       1 → "ssh-agent reachable but has no identities loaded"
       0 → parse output for the signingkey's fingerprint; absent → still
           an actionable error ("agent has keys, but not this one")
```
This is a genuinely different code path per format — a check written only
for the classic-GPG case (as a literal reading of CONTEXT.md's
`gpg-connect-agent` mention might suggest) would not catch the failure this
project's own release actually hit.

### Pattern 5: Second consumer of one on-disk gate record (20e)

**What (unchanged from CONTEXT.md D-01, verified against source and
confirmed correct):** `gates::Gates::respond` (`gates.rs:179-198`) writes
`NN-ship.response.json` unconditionally whether or not a live process is
polling for it — `respond` itself has no notion of "is anyone listening."
`pipeline_gate::run_gate`'s blocking `Gates::poll_response` loop
(`pipeline_gate.rs:208`) is the only current consumer. `devflow ship --phase
N [--force]` is a second, out-of-process consumer: read
`Gates::response_path` directly, convert via `GateAction::from_response`, and
on `Advance` call `finish_workflow(project_root, &mut state)`
(`pipeline_gate.rs:130-164`) — the exact same function `run_gate`'s
in-process `Advance` branch would have called via `handle_ship_outcome`
(`pipeline_outcomes.rs:282`). `finish_workflow` already handles the
lock-acquire-blocking, fail-closed retry-gate-reopen, and terminal
`workflow_finished` emission — nothing about it needs reimplementing for a
second caller.

**D-02's `--force` scope, confirmed against source:** the command must check
`state.stage == Stage::Ship` before doing anything (`workflow::load_state`
gives you `state.stage` directly) — any earlier stage should error out
directing the operator to resolve that stage first. `--force` only means
"skip re-verifying the Ship gate's own preconditions," never "advance past
Validate."

### Anti-Patterns to Avoid
- **Reintroducing a project-wide lock for `--until`'s stop check:** the stop
  interception is a pure `state.stage`/`state.stop_until` comparison — it
  needs no lock beyond what `transition`'s existing `workflow::save_state`
  call already uses.
- **Treating `devflow ship --force` as "skip Validate":** per D-02, `--force`
  only overrides the Ship-gate re-verification, never an earlier stage. A
  plan that lets `--force` short-circuit from any stage would violate the
  Phase 16 fail-closed terminal-Ship invariant Phase 17 explicitly
  regression-tested.
- **Fixing 20b instance 1 with only a fixture-side change:** the liveness
  gap in `cleanup` is real and user-reachable (see Pattern 2); a
  test-only retry/backoff wrapped around the fixture's own call would make
  CI green without fixing the product defect a real user can hit.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Phase liveness classification | A new "is this phase still active" predicate for `cleanup` | The existing `liveness()` (`commands.rs:371`) + `Liveness` enum from 18b | Already built, already tested (`liveness_matrix_covers_all_four_rows`), and doctor/status already depend on its exact semantics — a second implementation risks drifting from it (the documented failure class this project explicitly guards against elsewhere, e.g. `check_dead_monitor`'s doc comment "Reuses `liveness` rather than re-deriving the matrix, so the two copies can never drift"). |
| TOML section/value parsing for the self-pin fix | A hand-rolled parser from scratch | Extend `version.rs`'s existing `parse_section_header`/`find_version_in_contents`/`replace_version_in_contents` helpers | They already handle the exact section-scoped `key = value` scanning this fix needs; a parallel implementation duplicates logic this file already owns and tests (GAP-6 comment/quote preservation). |
| Ancestor-check for develop/main divergence (20d) | A custom git-log-walking algorithm | `git merge-base --is-ancestor origin/main HEAD` (already the exact command `scripts/sync-main-to-develop.sh` uses) | This project already has a working, tested command for this exact check — `devflow release --check` should shell out to the identical git invocation, not reimplement ancestor detection. |
| Gate response consumption for 20e | A new response-file schema or a poll loop | The existing `GateResponse`/`GateAction::from_response` (`gates.rs`) | D-01 explicitly locks this: one on-disk record, two consumers. A new schema would fork the protocol `18f`'s override already extends. |

**Key insight:** every one of these five units has an existing, tested
sibling mechanism in this codebase (liveness classification, TOML scanning,
ancestor-check scripting, gate-response protocol) — the correct shape in
each case is "extend/reuse," not "build a parallel implementation," and the
project's own code comments already say so in three of the five cases
(`check_dead_monitor`, `version.rs`'s GAP-6 tests, `sync-main-to-develop.sh`).

## Common Pitfalls

### Pitfall 1: Trusting CONTEXT.md's file paths without re-verifying
**What goes wrong:** CONTEXT.md's `<canonical_refs>` for 20a names
`crates/devflow-cli/src/version.rs`; the real file is
`crates/devflow-core/src/version.rs`.
**Why it happens:** The file was likely moved or was always in `core` and
the write-up was drafted from memory/summary rather than a fresh `find`.
**How to avoid:** The plan must cite `crates/devflow-core/src/version.rs`
directly; a task that tries to edit the CLI-crate path will fail outright.
**Warning signs:** Any task description or verification command referencing
`devflow-cli/src/version.rs` should be treated as a stale citation, not a
typo to route around.

### Pitfall 2: Assuming `check_stage_event_drift`/`check_missing_branch` cover the `--until` stop case
**What goes wrong:** These two checks are `Severity::Warn`, not `Problem`,
and don't fire for a clean `--until` stop by themselves — the dangerous one
is `check_dead_agent` (`Problem`), which WILL fire for any phase parked at
Define/Plan/Code with a dead recorded agent pid, exactly the state
`--until` leaves behind.
**Why it happens:** 18a's reconciliation checks were designed and tested
against "crashed mid-run" scenarios only; "intentionally stopped, by
design" wasn't a concept that existed until this phase.
**How to avoid:** Design 20c's stop path to leave state such that
`reconcile_phase` genuinely returns zero findings (or add a new, explicitly
non-`Problem` check that recognizes the stop marker) — do not treat "the
existing checks happen not to fire in my manual test" as proof; write a
`doctor`-integration regression test asserting zero `Problem`-severity
findings for a `--until`-stopped phase.
**Warning signs:** A plan that doesn't mention `check_dead_agent` at all
while implementing 20c is very likely to ship a false "stuck" diagnosis for
every single `--until`-stopped phase.

### Pitfall 3: `git worktree prune` mistaken for a delete-leftover-files operation
**What goes wrong:** Implementing 20b's fallback as "on remove failure, call
`prune`" leaves the actual leftover directory contents on disk while git's
own metadata believes the worktree is gone — a worse state than before (now
undiscoverable via `git worktree list`).
**Why it happens:** `prune`'s name and CONTEXT.md's phrasing ("fall back to
`git worktree prune`") both suggest a general cleanup operation; the git docs'
actual scope (administrative metadata only, and only for already-vanished
directories) is easy to skim past.
**How to avoid:** If retry-with-backoff on `git worktree remove --force`
still fails, either `fs::remove_dir_all` the leftover directory explicitly
before calling `prune`, or surface the failure to the operator rather than
silently calling `prune` and reporting success.

### Pitfall 4: Assuming instance-2's object-store corruption is purely a fixture concern
**What goes wrong:** CONTEXT.md's default lean ("no obvious product analog,
probably fixture durability") is reasonable but not proven, and this
research could not fully confirm or refute a product analog either —
`devflow parallel` runs multiple phases' agents committing concurrently into
worktrees that share one `.git/objects` store, with **no DevFlow-level lock
serializing those writes** (`lock::acquire_project`'s doc comment explicitly
scopes it to "the primary checkout" — version-bump/docs/branch-integration
— not arbitrary agent-driven commits inside per-phase worktrees). This is a
plausible, not confirmed, real-world analog.
**Why it happens:** The fixture's 60-commit loop is single-threaded within
the test itself; the corruption is attributed to shared-disk I/O contention
from *other, concurrently running CI test binaries* on the same runner, not
from concurrency inside the test. A real `devflow parallel` run under heavy
load is a structurally similar (shared object store, concurrent writers,
possible fsync-visibility lag) but distinct scenario.
**How to avoid:** Keep D-05's default (fixture-only fix: stronger
`core.fsyncObjectFiles`/`core.fsync` on fixture repos, and/or shrinking the
60-commit loop's window) as the phase's actual scope, but record the
`devflow parallel` shared-object-store question as an explicit open item for
future work rather than silently closing it as "definitely fixture-only."
**Warning signs:** A plan or verification step that asserts "confirmed
fixture-only" without addressing the `devflow parallel` concurrent-worktree
scenario is overclaiming past what this research could establish.

### Pitfall 5: Forgetting `help_snapshot.rs` when adding CLI surface
**What goes wrong:** `crates/devflow-cli/tests/help_snapshot.rs` diffs
`devflow --help` against a committed snapshot
(`tests/snapshots/devflow-help.txt`) and fails on ANY CLI surface change.
Both 20c (`--until` flag on `Start`) and 20e (new `Ship` subcommand) change
`--help` output.
**Why it happens:** Easy to miss since the failure only surfaces at `cargo
test`, not at compile time, and the fix (`cargo run -q -p devflow -- --help
> crates/devflow-cli/tests/snapshots/devflow-help.txt`) is a manual
regeneration step the test's own doc comment names.
**How to avoid:** Include an explicit task/step in both 20c's and 20e's
plans: regenerate the snapshot, and update `OPERATIONS.md`'s command table
(the test's own failure message directs to both).
**Warning signs:** `cargo test --workspace` failing only on
`help_output_matches_committed_snapshot` after an otherwise-clean CLI change.

## Code Examples

### Existing liveness predicate to reuse for 20b's `cleanup` fix
```rust
// Source: crates/devflow-cli/src/commands.rs:371 (verified at HEAD)
// Pure liveness predicate — no I/O. `monitor_pid` is matched `None` first
// so a state written by a pre-18b binary (carrying no `monitor_pid`) can
// never be misclassified as `Stuck` (T-18-11).
fn liveness(monitor_pid: Option<u32>, monitor_alive: bool, agent_alive: bool) -> Liveness {
    // ... None => Unknown; Some(pid) + monitor_alive => Healthy/BetweenStages; else Stuck
}
```

### Existing ancestor-check command for 20d, already proven in this repo
```bash
# Source: scripts/sync-main-to-develop.sh:41 (verified at HEAD)
git fetch origin main develop --quiet
git merge-base --is-ancestor origin/main HEAD   # exit 0 = already an ancestor (no-op)
```

### Existing gate-response protocol for 20e — read directly, no polling
```rust
// Source: crates/devflow-core/src/gates.rs:179-198, 208 (verified at HEAD)
// Gates::respond() writes the response file unconditionally (no "is anyone
// listening" check). pipeline_gate::run_gate's poll_response is the ONLY
// current reader — 20e adds a second reader of the same file:
let path = Gates::response_path(project_root, phase, Stage::Ship);
// if path.exists(): parse GateResponse, GateAction::from_response(&resp),
// on Advance -> pipeline_gate::finish_workflow(project_root, &mut state)
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| Manual per-release Cargo.toml edit for the workspace self-pin | `VersionBump` hook writes `[workspace.package] version` only (20a fixes the remaining gap) | Ongoing since v1.0 workspace split; last manually patched `7ad260c` (v1.5.0) and PR #15 (v1.6.0) | 20a removes the last manual-edit step in the release checklist that CONTRIBUTING.md's "Cutting a Release" section still implies is automatic. |
| `devflow cleanup --force` with no liveness awareness | (post-20b) liveness-gated removal | This phase | Removes a real, user-reachable data-loss/CI-flake vector, not just a test artifact. |
| Full-pipeline-only `devflow start` | `--until <stage>` (20c) | This phase | Makes cheap, frequent dogfood runs possible — CONTEXT.md's stated highest-yield bug source. |
| Manual release checklist (`CONTRIBUTING.md` § "Cutting a Release") | `devflow release --check` preflight (20d, read-only) | This phase | Converts four prose steps into structured, automatable checks; does not yet execute the release (that's explicitly out of scope per D-03). |
| `devflow gate approve` (requires a live monitor polling) | `devflow ship --phase N [--force]` (20e, works with a dead monitor) | This phase | Closes the "approval sits unconsumed forever" gap D-01 documents from source. |

**Deprecated/outdated:** none — no library or protocol in this phase is
being replaced, only extended.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `devflow parallel`'s concurrent per-worktree commits are a plausible (not confirmed) real-world analog for 20b instance 2's object-store race, since no DevFlow-level lock serializes them. | Common Pitfalls #4 | If wrong (no real analog exists), 20b instance 2 stays correctly scoped as fixture-only with no further action; if right and left unaddressed, `devflow parallel` under heavy concurrent load could intermittently produce genuine object-store corruption in a real user's repository — low likelihood, high severity if it occurs. |
| A2 | 20c's stop-marking mechanism should be a new `State` field (following the `#[serde(default)]` backward-compat pattern of every prior field addition), not a full `workflow::clear_state`. | Architecture Patterns, Pattern 3 | If wrong and the design should actually clear state entirely, `devflow status`/`doctor` would show nothing for a stopped phase (arguably simpler, but loses "what stage did this stop at" visibility CONTEXT.md's "persist a terminal-but-not-failed state" phrasing seems to want). This is a design decision for the plan/discuss step to confirm, not something this research can lock unilaterally. |
| A3 | The signing-viability check (20d) should special-case `gpg.format == "ssh"` using `ssh-add -l`'s three exit codes, based on this researched machine's actual git config. | Architecture Patterns, Pattern 4 | If the project's real release environment (not this research sandbox) uses classic GPG instead, the `ssh`-specific branch is simply unused code, not wrong code — the check should implement both branches regardless, so this risk is low. |

**Assumptions requiring explicit confirmation before locking into a plan:**
A1 (whether to file 20b instance 2's `devflow parallel` concern as a new
backlog item, per this phase's existing D-03-style precedent for
out-of-scope-but-real findings) and A2 (state-field vs. full-clear design
for 20c) should be raised explicitly, ideally during `/gsd-plan-phase`'s own
verification pass rather than assumed silently.

## Open Questions

1. **Does 20c's `--until` need a fourth stop target for "before Define even
   starts" (i.e., `--until define` meaning "just create the branch/worktree,
   don't launch an agent")?**
   - What we know: CONTEXT.md only discusses `--until plan`, `--until code`,
     `--until validate` as "equally reasonable" alongside plan.
   - What's unclear: whether `--until define` is a real use case or outside
     this phase's motivating scenario (which is specifically "stop after
     Plan produces PLAN.md files").
   - Recommendation: scope `--until` to accept any `Stage` value for
     consistency (it's the same enum `--stage` already uses elsevwhere), but
     the plan need not specially design for a "stop before Define" case
     distinct from just not running `start` at all.

2. **Should 20b instance 1's `cleanup` liveness guard be a hard refusal or a
   warn-and-force-anyway with `--force`?**
   - What we know: `cleanup --force` today already means "remove the
     reference worktree too" (an existing, different meaning of `--force`
     per `commands.rs:304`).
   - What's unclear: whether a second `--force`-shaped escape hatch is
     needed for "yes, I know a monitor might be alive, remove it anyway," or
     whether `cleanup` should simply refuse and direct the operator to
     `devflow resume`/wait, with no override.
   - Recommendation: this is a genuine product-behavior decision, not a
     research gap — flag for `/gsd-discuss-phase` or the plan's own
     acceptance criteria rather than resolving unilaterally here.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo`/`rustc` | All units (workspace build/test) | ✓ | cargo 1.97.1, rustc 1.97.1 | — |
| `git` | All units | ✓ | 2.55.0 | — |
| `ssh-add` (OpenSSH agent tooling) | 20d signing-viability check | ✓ (assumed present on any dev/CI machine with OpenSSH; not independently re-verified beyond `git config` inspection) | — | If absent, 20d's check should degrade to a clear "cannot verify signing viability — ssh-add not found" rather than a hard crash. |
| `gpg`/`gpg-connect-agent` | 20d signing-viability check (classic-GPG branch) | Not verified in this sandbox (repo uses `gpg.format=ssh`) | — | Same fail-soft requirement: absence of `gpg` tooling when `gpg.format != ssh` should produce an actionable message, not a panic. |
| GitHub Actions CI (`ubuntu-latest`, `cargo test --workspace`, no `--test-threads` limit) | 20b's flake reproduction context | ✓ (workflow files read directly) | `.github/workflows/ci.yml` | — |

**Missing dependencies with no fallback:** none identified.
**Missing dependencies with fallback:** `gpg`/`ssh-add` absence, handled
per above.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` / `cargo test`, workspace-wide |
| Config file | none (no `nextest.toml`/custom harness) — plain `cargo test --workspace` |
| Quick run command | `cargo test -p devflow-core <name>` / `cargo test -p devflow <name>` (no `--lib` — `devflow` is binary-only; `--lib` hard-errors, per the 18-01 decision recorded in STATE.md) |
| Full suite command | `cargo test --workspace` (currently 438+ tests, all green at research time) |

### Phase Requirement → Test Map

This project has no formal `REQ-ID` scheme (per ROADMAP.md/CONTEXT.md — units
are `20a`..`20e`, not numbered requirements). Mapping by unit instead:

| Unit | Behavior | Test Type | Automated Command | File Exists? |
|------|----------|-----------|--------------------|--------------|
| 20a | Workspace self-pin rewritten alongside `[workspace.package] version` | unit + existing guard | `cargo test -p devflow --test workspace_version_pin` (existing) + new unit tests in `crates/devflow-core/src/version.rs` | ✅ guard exists; ❌ new unit tests, Wave 1 |
| 20b (instance 1) | `cleanup --force` refuses/warns on a live phase's worktree | integration | new test in `phase7_cli.rs` or a new file | ❌ Wave 1 |
| 20b (instance 1) | `reference_and_cleanup_worktree_cli_flow` no longer flakes | existing integration, stabilized | `cargo test -p devflow --test phase7_cli reference_and_cleanup_worktree_cli_flow` | ✅ exists, needs the fix to make it non-flaky |
| 20b (instance 2) | `start_worktree_mode_ignores_main_checkout_divergence` no longer flakes | existing integration, stabilized (fixture durability) | `cargo test -p devflow --test phase7_cli start_worktree_mode_ignores_main_checkout_divergence` | ✅ exists |
| 20c | `--until plan` halts with no monitor, no `Problem` doctor finding | integration | new test, likely in `phase7_cli.rs` or a new `pipeline_stop.rs` | ❌ Wave 2 |
| 20d | `devflow release --check` flags self-pin drift, divergence, publish order, signing viability | unit (per-check) + one integration smoke test | new test file, e.g. `crates/devflow-cli/tests/release_check.rs` | ❌ Wave 2 |
| 20e | `devflow ship --phase N` advances via an already-written Ship response with no live process | integration | new test, likely in `pipeline_gate.rs`'s existing test module (mirrors `advance_ship_success_runs_finish_workflow`) | ❌ Wave 3 |
| 20e | `--force` refuses when `state.stage != Stage::Ship` | unit | new test alongside the new command handler | ❌ Wave 3 |

### Sampling Rate
- **Per task commit:** `cargo test -p <crate> <specific test name>` (no `--lib`)
- **Per wave merge:** `cargo test --workspace` (full suite, 0 failed required)
- **Phase gate:** Full suite green **on a pushed CI run**, not just local —
  this phase's own subject matter (20b) is CI-concurrency-dependent flakiness
  that does not reproduce reliably locally; local-green is explicitly
  insufficient for signing off on 20b specifically (mirrors the Phase 19
  `ENV_MUTEX` precedent: "Verification must be CI-on-branch — local-green is
  explicitly insufficient").

### Wave 0 Gaps
None — existing test infrastructure (`cargo test --workspace`, the
`phase7_cli.rs` integration harness, `ENV_MUTEX`/PATH-neutralization
patterns in `test_support.rs`) covers every test shape this phase needs; no
new framework or fixture scaffolding is required, only new test *files*
within the existing harness (see table above).

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | no | This phase has no user-facing auth surface. |
| V3 Session Management | no | N/A — CLI, not a session-based service. |
| V4 Access Control | partial | 20e's `--force` must remain scoped to `state.stage == Stage::Ship` only (D-02) — this is the closest thing to an access-control boundary in this phase, and it is enforced by a state-machine invariant, not a permissions system. |
| V5 Input Validation | yes | 20c's `--until <stage>` reuses the existing `Stage: FromStr` parser (`stage.rs:76-89`), which already rejects unknown stage names with a clear error — no new parsing surface. |
| V6 Cryptography | yes (read-only check, not a crypto implementation) | 20d's signing-viability check reads `git config`/`ssh-add` state; it must never read, log, or transmit private key material — only fingerprints/booleans (mirrors this codebase's existing `WR-02`-class discipline of never leaking filesystem paths/usernames into event logs, e.g. `commands.rs:1196`'s `PhaseFinding` doc comment: "Never carries a filesystem path or username"). |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| A `--force` flag silently widening scope over time (scope creep past its originally-intended boundary) | Elevation of Privilege | 20e's plan must include a regression test asserting `--force` still errors on any non-Ship stage — this is exactly the class of defect Phase 16/17 already hardened against for the terminal Ship invariant. |
| Leaking signing-key material or paths in 20d's preflight output | Information Disclosure | Only report boolean viability + key fingerprint (public data), never private key contents or full filesystem paths to key files — consistent with this codebase's existing WR-02 discipline. |
| A race in `cleanup --force` used as a way to force-remove another operator's in-progress work in a shared/multi-user checkout | Tampering | The liveness gate (Pattern 2) is itself the mitigation — it must be added, not merely documented as a known risk. |

## Sources

### Primary (HIGH confidence — read directly from repository source at researched HEAD)
- `crates/devflow-core/src/version.rs` — full read, `write_version`/`field_for`/`replace_version_in_contents`
- `crates/devflow-cli/tests/workspace_version_pin.rs` — full read
- `crates/devflow-core/src/hooks.rs` — `version_bump`, `changelog_append`, `hooks_after_ship`, `hooks_for_transition`
- `crates/devflow-core/src/gates.rs` — full read, `Gates::respond`/`poll_response`/`GateAction`
- `crates/devflow-cli/src/pipeline_gate.rs` — full read, `transition`/`finish_workflow`/`run_gate`
- `crates/devflow-cli/src/pipeline_launch.rs` — full read, `launch_stage`/`launch_stage_inner`/`advance`
- `crates/devflow-cli/src/pipeline_outcomes.rs` (partial) — `handle_validate_outcome`/`handle_ship_outcome`
- `crates/devflow-cli/src/commands.rs` (partial) — `cleanup`, `start`, doctor reconciliation (`Liveness`, `PhaseFacts`, `check_dead_agent`, `check_dead_monitor`, `reconcile_phase`)
- `crates/devflow-core/src/worktree.rs` — full read, `remove`/`prune`
- `crates/devflow-core/src/state.rs` — full read, `State` struct + serde-default pattern
- `crates/devflow-core/src/stage.rs` — full read, `Stage` enum + `next()`
- `crates/devflow-core/src/mode.rs` (partial) — `Mode::should_gate`
- `crates/devflow-cli/src/main.rs` — full read, `Command`/`GateCmd` clap enums, dispatch
- `crates/devflow-cli/tests/phase7_cli.rs` (partial) — both flaky test bodies + fixture helpers
- `crates/devflow-cli/tests/help_snapshot.rs` — full read
- `CONTRIBUTING.md` § "Cutting a Release" — full read
- `scripts/sync-main-to-develop.sh` — full read
- `OPERATIONS.md` (partial) — command table, gate section
- `.github/workflows/ci.yml`, `.github/workflows/devcontainer.yml` — full read
- `Cargo.toml` (workspace + both crates), `Cargo.lock` (toml/toml_edit presence check) — verified live
- `git config --get tag.gpgsign` / `gpg.format` / `user.signingkey` on the research machine — verified live via shell
- `cargo test --workspace` — executed live, 438+ tests, 0 failed, confirming a clean baseline before this phase's changes

### Secondary (MEDIUM confidence — official docs, cross-checked against source behavior)
- [git-scm.com/docs/git-worktree](https://git-scm.com/docs/git-worktree) — `remove`/`prune` exact semantics (clean-only removal, `--force` scope, `prune` is metadata-only for already-vanished directories)
- [Baeldung: Checking Active SSH Keys on Linux](https://www.baeldung.com/linux/active-ssh-keys) — `ssh-add -l` exit-code/output semantics (agent unreachable vs. no identities vs. listed)

### Tertiary (LOW confidence — general web results, not independently re-verified)
- [GitWorktree.org — remove tutorial](https://www.gitworktree.org/tutorial/remove) and [gitscripts.com — worktree remove reference](https://gitscripts.com/git-worktree-remove) — corroborating but non-authoritative summaries of the same git-scm.com behavior; cited only as secondary confirmation, not as the basis for any claim above.

## Metadata

**Confidence breakdown:**
- Version self-pin fix (20a): HIGH — read the exact function, the exact
  test guard, and the exact hook call site; fix shape is a mechanical
  extension of existing code with no open design questions.
- Fixture reliability (20b): HIGH on instance 1 (a concrete, previously
  undocumented product defect was found and confirmed by reading
  `cleanup`'s source directly); MEDIUM on instance 2 (CONTEXT.md's
  fixture-only default lean stands, but a plausible unconfirmed
  `devflow parallel` analog exists — see Assumption A1).
- Plan-only mode (20c): HIGH on the interception point (`transition`) and
  the doctor false-positive gap (both read directly from source); MEDIUM on
  the exact state-representation design (state field vs. full clear —
  Assumption A2), which is properly a plan/discuss-phase decision.
- Release-cut preflight (20d): HIGH on three of four checks (self-pin,
  divergence, publish order all have a directly-reusable existing
  command/pattern); HIGH on the `gpg.format=ssh` finding specifically
  (verified live against this repo's actual git config, not assumed).
- Manual ship override (20e): HIGH — CONTEXT.md's own D-01/D-02 design was
  independently re-derived from source during this research and found
  fully correct; no corrections needed.

**Research date:** 2026-07-22
**Valid until:** 30 days (stable internal codebase; re-verify source line
numbers if `main.rs`'s post-Phase-19 modules are touched by any
intervening phase before 20 is planned/executed).
