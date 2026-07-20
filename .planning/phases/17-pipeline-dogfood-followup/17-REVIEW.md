---
phase: 17-pipeline-dogfood-followup
reviewed: 2026-07-20T00:00:00Z
round: 5
depth: deep
angles:
  - doc-accuracy cross-reference
  - security / leaked-data
  - CI / build correctness
  - external-state claims
  - generalist deep pass
files_reviewed: 19
files_reviewed_list:
  - .github/workflows/ci.yml
  - .github/workflows/devcontainer.yml
  - OPERATIONS.md
  - crates/devflow-cli/build.rs
  - crates/devflow-cli/src/main.rs
  - crates/devflow-cli/tests/build_provenance.rs
  - crates/devflow-cli/tests/gitignore_coverage.rs
  - crates/devflow-cli/tests/log_format_env.rs
  - crates/devflow-cli/tests/snapshots/devflow-help.txt
  - crates/devflow-core/src/agent_result.rs
  - crates/devflow-core/src/agents/mod.rs
  - crates/devflow-core/src/git.rs
  - crates/devflow-core/src/hooks.rs
  - crates/devflow-core/src/lib.rs
  - crates/devflow-core/src/mode.rs
  - crates/devflow-core/src/outcome_policy.rs
  - crates/devflow-core/src/ship.rs
  - crates/devflow-core/src/state.rs
  - crates/devflow-core/src/version.rs
diff_base: a2c314f9076912cd4a525894ad5fd0b1aaa0d8b8
findings:
  critical: 2
  warning: 11
  info: 10
  total: 23
status: issues_found
ship_gate: BLOCKED
---

# Phase 17: Code Review Report — Round 5 (five-angle deep)

**Reviewed:** 2026-07-20 (round 5)
**Depth:** deep — five parallel angles, merged and deduplicated
**Files Reviewed:** 19
**Status:** issues_found — **2 Critical**
**Ship gate:** BLOCKED

Angles run: doc-accuracy cross-reference · security/leaked-data · CI/build correctness ·
external-state claims · generalist deep pass.

**Both Criticals were independently re-verified by the orchestrator** — CR-01 by a scratch
reproduction test (since removed; working tree confirmed clean), CR-02 by tracing the
recursion and reading the test fixture that documents and works around it.

Baseline at HEAD: `cargo test --workspace` 381 passed / 0 failed;
`cargo clippy --workspace --all-targets -- -D warnings` exit 0; `cargo fmt --check` exit 0.
**Every finding below is behaviour the suite does not cover.**

Round 4's three Criticals are all closed: R4-CR-01 (phantom `1.4.106` CHANGELOG heading)
triaged to ROADMAP 19d and no longer present in the tree; R4-CR-02
(`replace_version_in_contents`) fixed in `12b5b98`; R4-CR-03 (changelog/tag desync) fixed in
`e421ebd`. The two Criticals below are new findings, not carry-overs.

---

## Critical

### CR-01 — Layer 0 short-circuit makes the Validate stage unpassable whenever `external_verify` is declared

**Files:** `crates/devflow-core/src/agent_result.rs:784-796`, `crates/devflow-cli/src/main.rs:1354-1361`

17-03 removed `evaluate_layer0`'s `state.stage != Stage::Code` guard (D-05 gap 1 / D-06) and
added an affirmative-success arm that returns `verdict: None`:

```rust
// agent_result.rs:784 — every declared, approved probe passed
None => Some(AgentResult {
    status: AgentStatus::Success,
    ...
    verdict: None,
    decided_by_layer: Some(0),
}),
```

`evaluate_agent_result_inner` returns this immediately (`agent_result.rs:815`), so Layer 1's
`DEVFLOW_RESULT` marker — **the only carrier of a verdict** — is never read. `advance()` then
requires an explicit verdict at Validate:

```rust
// main.rs:1360
let passed = matches!(result.verdict, Some(Verdict::Pass));
handle_validate_outcome(project_root, &mut state, passed)
```

Layer 0 Success and `verdict: None` compose into `passed = false`.

**Failure scenario (reproduced).** A PLAN declares `external_verify: "test -f shipped"`, the
operator approves via `DEVFLOW_TRUST_EXTERNAL_VERIFY`, the probe passes, and the agent emits
`DEVFLOW_RESULT: {"status":"success","verdict":"pass"}`. Scratch test at the cascade level:

```
REPRO status=Success layer=Some(0) verdict=None
REPRO advance() passed = false
```

The agent's explicit `verdict: pass` is discarded. Validate can never reach Ship. In Supervise
mode this fires a spurious "Validation failed" gate every cycle; in Auto mode it loops
Code↔Validate — and because `transition` unconditionally resets `consecutive_failures = 0`
(`main.rs:1797`, see WR-11 / ROADMAP 19g), the `MAX_CONSECUTIVE_FAILURES` escape hatch never
fires, so the loop is **unbounded**.

`config::external_verify_enabled` defaults to **`true`**, so only the PLAN declaration is
opt-in. No PLAN in this repo currently declares `external_verify`, which is why the suite is
green. The two existing Layer 0 cascade tests
(`layer0_affirmative_success_on_non_code_stage_with_zero_commits`,
`layer0_affirmative_success_outranks_layer1_failure_marker`) assert `status`,
`decided_by_layer`, and `commits` — but **never `verdict`, and never at `Stage::Validate`**.

**Fix:** either have Layer 0 defer to Layer 1 when `state.stage == Stage::Validate` so the
agent's verdict is still consulted, or set `verdict: Some(Verdict::Pass)` on the
affirmative-success result at Validate. Add a cascade test pinning `verdict` at Validate.

---

### CR-02 — Approving a preflight gate re-runs the identical failing check, wedging the phase for the 7-day gate timeout

**File:** `crates/devflow-cli/src/main.rs:796-828` (`run_preflight`), reached from `launch_stage:1167`

```rust
match run_gate(project_root, state, stage, &context)? {
    GateAction::Advance => {
        let _ = Gates::cleanup(project_root, state.phase, stage);
        state.gate_pending = false;
        launch_stage(state, None, None)?;      // -> run_preflight again
    }
    GateAction::LoopBack(_) => {
        let _ = Gates::cleanup(project_root, state.phase, stage);
        launch_stage(state, None, None)?;      // -> run_preflight again
    }
```

Both generic checks are **deterministic, idempotent predicates over state that a gate approval
cannot change** — `preflight_interactivity_check` (`main.rs:722`) tests
`phase_artifact_on_develop(.., "-CONTEXT.md")`; `preflight_gh_auth_check` (`main.rs:755`)
shells `gh auth status`.

**Failure scenario.** `state.stage == Stage::Ship`, `gh` installed but the token expired.
`launch_stage` → preflight fails → gate fires → operator approves. `Gates::cleanup` **deletes
the response**, `launch_stage` recurses, `run_preflight` re-runs `gh auth status`, it fails
identically, `run_gate` writes a second gate and calls
`Gates::poll_response(.., gate_timeout_secs())` — defaulting to **7 days** (`main.rs:31`). The
operator's response was just deleted, so the detached monitor blocks for a week, then returns
`Err("gate for stage Ship timed out awaiting a response")`, leaving `gate_pending: true`
persisted (`run_gate:1901` saves `true`; the `None` timeout arm never resets it). The same
applies to `resume --phase N` on a Codex/Auto/Define phase with no CONTEXT.md, which bypasses
`start()`'s pre-state hard check by design.

**The codebase already knows.** `FailOnceAdapter`'s own doc comment (`main.rs:4962-4967`)
documents the wedge and works around it:

> "An adapter that fails unconditionally would make a recursive `launch_stage` retry fail its
> OWN preflight check too, recursing into a second gate this test never seeds a response for —
> blocking on `poll_response` instead of asserting."

`run_preflight_advance_gate_launches_agent_exactly_once` (`main.rs:5065`) and its LoopBack twin
pass **only** because the fixture returns `Ok(())` on its second call. No test drives the
recursion with an idempotently-failing check — which is exactly how both production checks
behave.

**Fix:** make the retry bypass the check that already gated (thread a "preflight already
adjudicated" flag through `launch_stage`, mirroring how `handle_stage_failure` passes
`Some(stage)` as `archived_stage`), or treat `GateAction::Advance` on a preflight gate as an
explicit override that skips `run_preflight`. At minimum, bound the recursion so a second
identical failure aborts instead of re-polling for 7 days.

---

## Warning

### WR-01 — `docs_update`'s `git add .` can commit raw agent stdout in target projects

**Files:** `crates/devflow-core/src/hooks.rs:184`, `crates/devflow-core/src/git.rs:308-319`

`docs_update` is the only remaining `commit_all` caller and runs `git add .` at
`ctx.project_root` — the **user's** repo. `.devflow/phase-NN-stdout` is raw, unredacted agent
stdout. If a target project's `.gitignore` lacks `.devflow/`, the Validate→Ship `DocsUpdate`
hook sweeps it into a commit that `Merge` then pushes. Reproduced in a scratch repo:

```
$ git add . && git commit -qm init && git log -1 --name-only --pretty=format:
.devflow/events.jsonl
.devflow/phase-01-stdout
README.md
```

The assumption is asserted in test fixtures (`hooks.rs:489-491`, `:552-554`: "gitignored in
every real project (WR-11)") but **enforced nowhere**. Both guards (`gitignore_coverage.rs`,
`doc_check.rs:283`) only cover DevFlow's own repo. Pre-existing, but this phase introduced
`commit_path` to avoid exactly this sweep for `ChangelogAppend`/`VersionBump` and left
`docs_update` behind.

**Fix:** scope `docs_update` via the new `commit_path`, or have `lock::ensure_devflow_dir`
write a `.devflow/.gitignore` containing `*` on creation — self-ignoring, requires no change
to the user's root `.gitignore`, and closes it for every constructor at once.

### WR-02 — `exe_path` provenance field writes the developer's absolute home path and OS username into `.devflow/events.jsonl`

**File:** `crates/devflow-cli/src/main.rs:200-212` (new this phase, D-21)

`"exe_path": std::env::current_exe()...` resolves to e.g.
`/var/home/<user>/.../target/debug/devflow`. Appended on every `devflow start`;
`OPERATIONS.md` advertises `events.jsonl` as a file to "tail from any tool", so it is
routinely read and pasted. Combined with WR-01 it becomes committable in a target project.
Gitignored in DevFlow's own repo (verified: `git ls-files .devflow` → 0 tracked files).

**Fix:** emit only `current_exe().file_name()`, or a path relative to `project_root`.
`DEVFLOW_BUILD_COMMIT`/`DEVFLOW_BUILD_DIRTY` already carry the diagnostic value.

### WR-03 — `commit_path`'s `--allow-empty` creates spurious empty commits and makes its own error arm dead code

**File:** `crates/devflow-core/src/git.rs:325-345`

The doc says "Returns Ok(()) whether or not the path had changes to commit", but
`--allow-empty` does not *skip* — it **commits**. Verified in an isolated repo:

```
$ git commit --allow-empty -m "empty test" -- a.txt   # a.txt unchanged
[master 7f1c5c8] empty test
```

If `version_bump` (`hooks.rs:239-243`) re-runs after a fail-fast retry of the terminal batch
and `write_version` produces byte-identical content, an empty `chore: bump version to X`
commit lands on develop and the release tag is placed on a commit containing nothing. Because
`--allow-empty` suppresses the failure entirely, the `msg.contains("nothing to commit")` guard
arm can never be taken — dead code that reads as if it were the skip path.

**Fix:** drop `--allow-empty` and keep the `nothing to commit` arm as the genuine no-op path,
or check `git diff --cached --quiet -- <path>` before committing.

### WR-04 — Layer 0's unapproved-probe veto expanded from one stage to all five

**File:** `crates/devflow-core/src/agent_result.rs:725, 744-757`

With the `Stage::Code` guard removed, a PLAN carrying `external_verify:` without
`DEVFLOW_TRUST_EXTERNAL_VERIFY` exported (or not inherited by the detached monitor subprocess)
now returns `Failed` → `Action::GateReview` at Define, Plan, Code, Validate **and** Ship. With
`external_verify_enabled` defaulting to `true`, the blast radius of a forgotten env var grew
5×. Documented as intentional (D-05 gap 1), so this is a deliberate trade rather than an
oversight — but no test covers the unapproved path at a non-Code stage, and the env-var
inheritance requirement for the detached monitor is not called out in `OPERATIONS.md`.

### WR-05 — `17-VERIFICATION.md` claims a full-coverage pass "at current HEAD" that predates 17-13

**File:** `.planning/phases/17-pipeline-dogfood-followup/17-VERIFICATION.md:2-6, 35-37`

Frontmatter reads `status: passed`, `score: 15/15 must-haves verified`, and the body claims to
be "the first verification pass to cover the full 17-01..17-12 plan set at current HEAD". But
`ROADMAP.md:196` now reads **13/13 plans executed**, and 17-13 landed three `crates/` commits
after that pass (`12b5b98`, `e421ebd`, `0189808`). `git log -- 17-VERIFICATION.md` → last
touched at `f5c399a`, 12 commits before HEAD. Its recorded test counts are measurably stale
(284/70 recorded; 288/71 at HEAD).

Everything is green at HEAD, so this is a false *coverage* claim, not a hidden failure — but it
is the phase's authoritative gate artifact and it reads as covering code it never saw.

**Fix:** re-run verification at HEAD, or scope the report to `f5c399a` explicitly and mark
17-13 unverified.

### WR-06 — ROADMAP items 19e and 19f describe defects 17-13 already fixed

**File:** `.planning/ROADMAP.md:259-260`

19e (`replace_version_in_contents` drops the trailing comma) is closed by `12b5b98` —
`version.rs:284-307` now captures and re-emits the remainder. 19f (`hooks_after_ship()`
changelog/tag desync) is closed by `e421ebd` — `hooks.rs:49,205,255` thread `shipped_version`.
Both have RED-proven passing tests
(`write_version_preserves_trailing_comma_in_package_json`,
`after_ship_batch_with_no_version_file_keeps_tag_and_changelog_in_sync`). 19e also still says
"Pre-existing; unrelated to Phase 17's changes", now false. ROADMAP is this repo's authoritative
scheduling record — whoever plans Phase 19 will re-open two closed defects and may "fix"
already-correct code.

### WR-07 — `cargo test --workspace` is not deterministically green (`build_provenance` flake), and neither job has a timeout

**Files:** `crates/devflow-cli/tests/build_provenance.rs:148-210`, both workflow files

Five workspace runs: run 1 aborted (`could not execute process build_provenance-... (never
executed) / No such file or directory`); run 2 →
`build_dirty_flips_false_to_true_across_a_working_tree_edit_after_rebuild ... FAILED`
(`test result: FAILED. 2 passed; 1 failed`); runs 3-5 green. Isolated
(`-p devflow --test build_provenance`) → 3 passed in 52s. The test copies 374 tracked files,
`git init`s, and runs a full `cargo build` twice into a fresh target dir — measured 48-92s and
~1.2 GB on 12 cores, contending with the always-rerun `build.rs`. On a 2-core `ubuntu-latest`
runner, cold and concurrent with eight other test binaries, this is realistically several
minutes.

Neither workflow sets `timeout-minutes`, so the GitHub default of **360 minutes** applies. A
single green run is not evidence the gate is stable, and `17-VALIDATION.md:1240`'s
"381 passed / 0 failed" is one sample.

**Fix:** add `timeout-minutes: 30` to both jobs; serialize or cap the nested build (`--jobs 2`).

### WR-08 — The phase's own clippy-scope fix has no regression guard

**Files:** `.github/workflows/ci.yml:30`, `.github/workflows/devcontainer.yml:26`

Verified the widening matters — injecting `format!("{}", (&r).to_string())` into a
`#[cfg(test)]` module: `cargo clippy -- -D warnings` → exit 0, zero diagnostics;
`cargo clippy --workspace --all-targets -- -D warnings` → exit 101. The only workflow-asserting
test (`devcontainer_ci_failfast.rs:59`) checks `set -e` ordering only and never opens `ci.yml`.
A revert of line 30 goes green while silently no longer compiling any test target — the exact
defect class WR-05/WR-06 just fixed, on a phase that added ~2,670 lines to `main.rs`.

**Fix:** assert that the clippy line in **both** workflow files contains `--workspace` and
`--all-targets`; the `run_cmd_lines` parsing machinery already exists.

### WR-09 — `CONTRIBUTING.md` claims to mirror `ci.yml`; this phase changed `ci.yml` and left it behind

**File:** `CONTRIBUTING.md:98-102` (also `:43`, `:93`)

States the required-check list "mirrors `.github/workflows/ci.yml`", then lists
`cargo clippy -- -D warnings`, while `ci.yml:30` is now `--workspace --all-targets`. An
explicit "mirrors" claim, now false in the stricter direction: a contributor who runs the
documented command sees clean output and gets a red CI on any warning inside a `#[cfg(test)]`
module.

### WR-10 — `devflow test`, documented as *the* local quality gate, runs the pre-phase-17 clippy invocation

**File:** `crates/devflow-cli/src/main.rs:3084`

```rust
("cargo clippy", "cargo clippy -- -D warnings"),
```

`OPERATIONS.md:45` and `README.md:116` present `devflow test` as the pre-push gate standing in
for CI. Commit `50a6b16` updated both workflow files and neither the CLI nor CONTRIBUTING — so
the tool DevFlow ships as the pre-push gate is now weaker than the CI it represents.

**Fix:** align `test_cmd`'s clippy entry with the CI invocation.

### WR-11 — `mode.rs` documents a Validate failure bound that the code cannot reach

**File:** `crates/devflow-core/src/mode.rs:7-8`

> "The only human gate is at Ship — unless Validate fails `MAX_CONSECUTIVE_FAILURES` times in
> a row, which forces a gate."

A failed Validate increments `consecutive_failures` (`main.rs:1511`) then loops back via
`loop_back_to_code` → `launch_stage`. The next Code completion hits `main.rs:1353` →
`transition`, which unconditionally does `state.consecutive_failures = 0` (`main.rs:1797`). The
counter oscillates 0↔1, so `should_gate`'s `>= MAX_CONSECUTIVE_FAILURES` (`mode.rs:58`) never
fires. Known as ROADMAP 19g ("observed live across three cycles") — confirmed at source.
`mode.rs` is in this phase's diff and its doc block was edited here (the `MAX_INFRA_FAILURES`
addition) without correcting the adjacent false claim.

The module doc is the spec for Auto mode's only safety valve, and the failure mode is an
indefinite unattended loop that `status` renders as healthy. **This is also the amplifier that
makes CR-01's Auto-mode loop unbounded.**

**Fix:** land 19g's fix, or annotate lines 7-8 with the known-unreachable caveat and an issue
reference until it lands.

---

## Info

- **IN-01** — `build.rs:43-45` declares the old `HEAD`/`refs`/`packed-refs` `rerun-if-changed`
  list while its own module doc (13-32) and inline comment (37-42) describe the single
  never-exists sentinel path. Behaviour is correct (the always-rerun path dominates, proven by
  mutation below), but the declaration contradicts the docs.
- **IN-02** — Round 4's frontmatter read `ship_gate: BLOCKED` / `critical: 3` after all three
  were resolved, with no disposition block (unlike rounds 1-3). Superseded by this rewrite.
- **IN-03** — No `permissions:` block in either workflow; both inherit the default
  `GITHUB_TOKEN` scope. No secrets referenced, no `pull_request_target`, no
  `${{ github.event.* }}` reaching a `run:` block — so no injection path — but
  `dtolnay/rust-toolchain@stable` and `devcontainers/ci@v0.3` are floating, non-SHA-pinned
  third-party actions. Add `permissions: contents: read`.
- **IN-04** — `ship.rs:409-424` `shell_quote` treats `~` as safe-unquoted, so the stated
  invariant "widening this list can never under-quote" is not strictly true (a leading `~`
  undergoes tilde expansion). Not currently exploitable — the only value passed is an absolute
  `project_root`.
- **IN-05** — `embedded_commit_is_stale`'s ~25-line doc block (`main.rs:~840`) sits above
  `#[derive] enum Staleness`, so `cargo doc` renders a function's contract (including the
  exit-code table) as the enum's docs; the function itself has none. Repeats at `~1063` for
  `staleness_outcome`.
- **IN-06** — Breaking public API change at a patch version: `Hook::run` changed from
  `&HookContext` to `&mut HookContext`, and `HookContext` gained a public field
  (`hooks.rs:44,68`). Neither crate sets `publish = false`; the workspace moved only within
  `1.3.x`.
- **IN-07** — `parse_version_str` (`version.rs:~180`) pads missing components to `0` via
  `parts.next().unwrap_or("0")`, so `version = "1.2"` parses as `1.2.0` rather than erroring.
  `read_version` feeds the changelog heading, so a malformed manifest is normalised rather than
  surfaced.
- **IN-08** — `hooks_after_ship()` (`hooks.rs:105-112`) orders `VersionBump` (which tags)
  before `ChangelogAppend` (which commits), so the tagged commit never contains its own release
  entry. Deliberate (WR-04 reordering) and asserted by test, but anyone checking out `vX.Y.Z`
  gets a `CHANGELOG.md` missing its own heading.
- **IN-09** — `devflow resume` is absent from every README command table despite being the
  user-facing recovery verb this phase added (present in `OPERATIONS.md:32` and the help
  snapshot). `doc_check.rs:415-422` does not catch it because it concatenates all scoped docs
  before matching, so `OPERATIONS.md` alone satisfies the guard. ROADMAP 19c independently names
  recovery-verb discoverability as a live operator complaint.
- **IN-10** — Doc/code drift, low impact: `README.md:87` scopes rate-limit detection to
  `sequentagent` though D-09 added it to the primary loop (`main.rs:1432-1458`);
  `ARCHITECTURE.md:147-149` and `README.md:85` describe Layer 3 as Unknown-only, omitting
  17-03's fail-closed `Failed` branch (`agent_result.rs:675-690`); `17-VALIDATION.md:39,46`
  calls the narrow clippy command "CI-parity"; `CHANGELOG.md` credits `devflow doctor` with
  stale-binary detection that actually lives in `enforce_build_staleness`; `build.rs:16-18`
  cites `SystemTime::now()` as an input the same commit removed; `ROADMAP.md:5-15` regressed
  Phase 17 to "In Progress" while `STATE.md:49` lists it Completed, and omits Phase 19 from the
  table; `17-VALIDATION.md:21`'s `reaudited_10_at_commit: eda94cd` is one commit behind the
  state it audited (`0189808`); `17-REVIEW.md` R4's "`total_plans: 46` is correct" was true when
  written but reads as present tense (now 47).

---

## Verified clean

**Security.** No secrets, credentials, tokens, session data, or PII are committed by this phase.
Zero HTTP clients in the dependency tree (`reqwest`/`hyper`/`ureq`/`curl` all absent) — no
telemetry. `preflight_gh_auth_check` branches on `output.status.success()` only and never
captures, logs, or prints `gh auth status` stdout/stderr, honouring its own T-17-13 constraint.
No `GH_TOKEN`/`GITHUB_TOKEN`/`ANTHROPIC_*` reference anywhere in `crates/` or `.github/`. No
command injection: the one `sh -c` on an attacker-influenceable path (`gates.rs:296`
`run_notify_command`) passes gate metadata — including agent-generated `context` — to the child
as **environment variables**, not interpolated into the command string; `commit_path` and all
phase interpolations pass argv elements or `u32` phase numbers. `build.rs` embeds only
`DEVFLOW_BUILD_COMMIT` (a SHA) and `DEVFLOW_BUILD_DIRTY` (a bool) — no paths, usernames,
hostnames, branch names, or diff content, and it uses argv-array `Command` throughout.

**CI false-green hypotheses tested and ruled out.** build.rs trigger-set regression (proven by
mutation — reverting `build.rs:43` makes the provenance test FAIL, so it is not vacuous); bare
`cargo test` vs `--workspace` (both produce 9 executables — textual divergence only, prior
IN-07); pipes without `pipefail` (no `|`/`tee`/`grep` in any `run:`/`runCmd:` block);
`continue-on-error` / `|| true` / `if: always()` (zero hits across `.github/`);
`cargo test --exact` matching zero tests (zero occurrences repo-wide); matrix leg masking (no
`strategy:` in either workflow); caching serving a stale binary (no `actions/cache` or
`rust-cache`; cold build every run); tests silently skipped via env var/feature/`#[ignore]`
(381 passed / 0 failed / 0 ignored / 0 filtered); fixtures failing without a CI git identity
(all set `user.email`, `user.name`, `commit.gpgsign false`); test helpers swallowing git
failures (all `assert!(output.status.success())`); unbounded polls (bounded at 200×25 ms with
`panic!` on timeout); snapshot test regenerating its own snapshot (`include_str!` +
`assert_eq!`, never writes); `cargo fmt --check` present in both workflows.

**`gitignore_coverage.rs` hardening is genuine.** The per-path loop replacing the batched
`check-ignore` argv is a real fix: `git check-ignore` exits 0 when *any* argument matches, so the
old batched call would have stayed green while individual paths lost coverage. All 14
`RUNTIME_PATHS` are covered, and the negative control fails correctly.

**External-state claims — no false merge/tag/release/deletion assertions.** No artifact claims
Phase 17 is merged, PR'd, pushed, tagged, or released. `git branch --merged develop` → `develop`,
`main` only. `git ls-remote --heads origin` → no `feature/phase-17`. `gh pr list --state all`
(authenticated) → exactly PRs #1-#10, all MERGED, none for Phase 17. `git tag -l` tops out at
`v1.3.69`, matching `Cargo.toml:9`; `git ls-remote --tags origin` agrees. CHANGELOG contains no
unbacked release heading. `STATE.md`'s `total_plans: 47` verified by counting plan files
(12+6+4+5+7+13). Phase 14's merge claim (`431c743`) confirmed an ancestor of `origin/develop`.
`17-VALIDATION.md:1237`'s "eight commits in `04a5e55..eda94cd`, three touching `crates/`" is
exact.

**Note for whoever ships.** Local `develop` is 18 commits ahead of `origin/develop`, and the
merge-base `a2c314f` is *not* an ancestor of `origin/develop`. Phase 17's own 17-01/17-02 sit in
that unpushed range. No artifact asserts otherwise, but `develop` should be pushed first.

**Not verifiable from here.** `ROADMAP.md:144` / `STATE.md:120` "both crates confirmed live on
crates.io" — the crates.io API returned a data-access policy error, not a 404. Phase 15 claim,
outside this diff range.

---

## Ship gate

**BLOCKED — 2 Critical.**

CR-01 and CR-02 are both latent: they require an `external_verify` declaration and a failing
preflight check respectively, neither of which occurs in this repo today — which is why 381
tests pass. But both are reachable on the documented operator paths this phase exists to harden,
and both fail toward an *unbounded wait* rather than a loud error: CR-01 loops Code↔Validate
forever (WR-11 removes the bound), CR-02 blocks on a 7-day poll after the operator has already
responded.

Fix CR-01 and CR-02, add the two missing tests (verdict-at-Validate; preflight retry with an
idempotently-failing check), then re-run `/gsd-code-review 17` before shipping.
