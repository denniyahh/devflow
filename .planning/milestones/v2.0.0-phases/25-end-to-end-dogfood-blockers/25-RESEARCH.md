# Phase 25: End-to-End Dogfood Blockers — Start, Progress, Finish, Recover - Research

**Researched:** 2026-07-27
**Domain:** Rust CLI internals (git plumbing, process management, preflight/gate machinery) — no new external services, no new runtime network dependency
**Confidence:** HIGH for 25c's git plumbing and 25b/25e/25f mechanics (all verified live against this repository's actual history and source); MEDIUM for 25c's preflight-timing question and 25d's SIGTERM-immunity root cause (both are genuinely open, and this document says so plainly rather than papering over them); MEDIUM for 999.38 (root cause reasoned from source, not reproduced live)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Scope corrections:**
- **D-01:** The ROADMAP's 25b sizing rationale is wrong — `archived_stage: Option<Stage>` is `None` at *three* call sites, not one (`commands.rs:236` fresh start, `pipeline_launch.rs:233` resume-after-pause, `preflight.rs:435` LoopBack retry), so the naive one-liner (`if archived_stage.is_none()`) would still hard-block a resumed mid-run self-modifying phase. Do not implement the ROADMAP's proposed one-liner.
- **D-02:** The ROADMAP's 25e framing is wrong — the "dangerous production guard" already moved to `(pid, starttime)` identity (`lock.rs:123-181`, `commands.rs:1191-1200`). `looks_like_devflow_process` has **no production callers** left. 25e is dead-predicate/flaky-test cleanup, not a live-guard hardening.

**25b — staleness pin (999.48):**
- **D-03:** Hoist `enforce_build_staleness` out of `launch_stage` (`pipeline_launch.rs:93`) into `commands.rs`, immediately before `launch_stage(&mut state, None, None)` at line 236 — **after** `state.worktree_path` is set (line 199). No new `State` field, no pin/mismatch semantics. Chosen on explicit operator simplicity-tiebreak ("I don't want to overengineer this since this is only to support dogfooding").
- **D-04 (accepted trade):** Under D-03, `resume` no longer re-checks staleness mid-run. Accepted — matches the operator's standing "only validated and pushed code should ever be used" position.
- **D-05 (non-negotiable):** Do not re-propose either of 999.48's rejected alternatives (mid-run binary rebuild; a dogfood bypass flag). Do not weaken D-18 generally — only its per-stage re-check scope.

**25c — versioning (999.49):**
- **D-06:** The June 2026 commit-message-versioning ban is lifted. `.planning/ROADMAP.md:36` and `.planning/PROJECT.md`'s Constraints section must both be updated in this phase, or a verifier will treat D-07 as a violation of a stale constraint. **Reversibility: costly** — crates.io versions can never be reused.
- **D-07:** Baseline = the highest reachable semver tag. Enumerate `git tag`, keep only `v?MAJOR.MINOR.PATCH` values, keep only those reachable from `HEAD`, take the max by **semver ordering** (not string sort, not count). **No `git describe` anywhere.** Verified 2026-07-27: `v1.0.1`…`v2.0.0` are all reachable from `HEAD`; the only non-reachable tag is `archive-planning-docs-2026-07-24` (non-semver). Resolves to `v2.0.0` today.
- **D-08:** Bump = conventional-commit classification over `--no-merges` commits in `baseline..HEAD`. `!` or a `BREAKING CHANGE:` footer → major; `feat` → minor; `fix`/`perf` → patch; `docs`/`test`/`chore`/`ci`/`refactor`/`style` → no bump. Highest precedence wins. Measured: 118/120 of the last non-merge commits conform to `type(scope): subject`.
- **D-09:** A major bump opens a gate; it never ships unattended. Detection runs as a **named preflight check** inside `run_preflight` (`crates/devflow-cli/src/preflight.rs`), reusing the existing gate+notify machinery, bounded by `preflight_retries`. **Placement is load-bearing**: must be evaluated **before `hooks_after_ship` runs at all** (Merge→VersionBump→ChangelogAppend→BranchCleanup is a no-rollback fail-fast batch, and a gate that opened inside `VersionBump` would fire after Merge already committed).
- **D-10:** Floors: a range where nothing bumps → **bump patch anyway** (every ship yields a distinct version). An unrecognised/malformed commit type → **treat as patch**. The highest semver tag overall is **not** reachable from `HEAD` → **refuse**, naming the unreachable tag and the repair command — do NOT silently fall back to the highest *reachable* tag.
- **D-11:** `Cargo.toml` stops being a version-computation *input* — it becomes purely an *output* `VersionBump` writes. Collapses the two-places-to-bump drift. `read_version` keeps its distinct role (reports what's on disk, never touches git) and must not be conflated with this path.
- **D-12 (coupling, record don't silently depend on):** D-07's correctness depends on 999.52's sync discipline (a squashed `develop`→`main` sync PR breaks ancestry and regresses the baseline). D-10's refuse-on-unreachable is the mitigation. 999.52 stays in the backlog, not in this phase.

**25e — dead predicate (999.47):**
- **D-13:** `#[deprecated]` on `agent::looks_like_devflow_process`; rewrite both flaky tests (`agent::tests::looks_like_devflow_process_is_false_for_a_non_devflow_process`, `commands::tests::stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check`) to assert the `(pid, starttime)` identity guard production actually uses. The retarget removes the fork/exec race by construction (no `spawn()` needed to test a starttime mismatch). **Deletion was considered and rejected** — the crate has no `publish = false`, `lib.rs:54` is `pub mod agent`, so removing a public fn of a published crate would trip the D-08 breaking classifier and spend the reserved `3.0.0` slot on a function with zero known callers. Rides the next real major instead.

**999.38 — folded into 25b (not 25e):**
- **D-14:** Fix `staleness::tests::ahead_build_from_descendant_commit_warns_instead_of_blocking` (`staleness.rs:891`) alongside 25b's work in the same module. Fix direction per the backlog entry: per-`Command` `env`/`env_remove` (as `test_support::git_command`/`hermetic_command` already does for `GIT_DIR`-class vars), **not** process-global `set_var`/`remove_var` — Rust 2024 marks those `unsafe` precisely because they are unsound in a threaded test binary.

**Acceptance:**
- **D-15:** The end-to-end acceptance run is unofficial and continuous — it gates **no phase's completion**, until further notice. Phase 25 is complete when 25a–25f are each independently verified on unit-level merits.
- **D-16:** The ROADMAP's Phase 25 "Acceptance" paragraph must be rewritten to match D-15 — folded into 25f.

**25a and 25d — not discussed, ROADMAP directions stand as written:**
- **D-17:** 25a's three options (fetch-before-resolve / resolve against `origin/develop` / compare-and-refuse-loudly) are left **unresolved** by the operator — the planner receives all three, unchosen, with the binding constraint that "heading present but code stale" must be closed, not just "heading absent." 25d: registry-independent PID discovery, safe on a shared machine; **must** escalate `TERM`→`KILL` with a bounded, death-verifying wait; **must** reap the wrapper/child pair together; census must be **subreaper-aware** (these processes reparent to `systemd --user`, not pid 1).

### Claude's Discretion

- Sequencing within the phase (ROADMAP's spine 25a→25b→25c stands; 25d/25e parallel).
- Where the semver-tag parsing/reachability predicate lives (`version.rs` vs. a new helper), and whether an existing crate is used for semver ordering vs. hand-rolled — the zero-network-deps constraint is about runtime network calls, not a parsing dependency, but keep the dependency budget in mind.
- The exact shape of 25d's discovery mechanism, within D-17's constraints.

### Deferred Ideas (OUT OF SCOPE)

- A `commit-msg` hook enforcing Conventional Commits — expands the phase beyond its six units; D-10's "unrecognised → patch" makes the classifier safe without it.
- Reporting unrecognised-commit counts in ship output — not chosen; D-10 took "treat as patch" instead.
- Deleting `looks_like_devflow_process` outright — deferred to the next real major.
- Deleting `staleness.rs` entirely — considered and rejected; the outstanding work is one moved call.
- 999.52 (branch-model repair subcommand) — named as a coupling (D-12), stays in backlog.
- 999.45, 999.43, 999.46, 999.50 — open, out of scope, untouched.
- Reinstating the end-to-end run as a phase gate — D-15 suspends it "until further notice," deliberately reversible.
</user_constraints>

<phase_requirements>
## Phase Requirements

No formal `REQUIREMENTS.md`/REQ-IDs exist in this project (confirmed: `.planning/PROJECT.md` "No `.planning/REQUIREMENTS.md` exists in this project"). Requirements are the six lettered units below, each mapped 1:1 to a filed backlog item.

| ID | Description | Research Support |
|----|-------------|------------------|
| 25a | A run starts on a current base ref (999.51/DEN-76) | Priority-3 brief below: `origin_main_ancestor_status` pattern (`git.rs:462-508`) as a directly reusable template for whichever of the three unresolved options the planner picks |
| 25b | A run progresses past the Validate boundary on a self-modifying phase (999.48/DEN-73) | Priority-3 brief below: exact hoist mechanics, import/visibility check, existing test impact |
| 25c | A run finishes with a correct version (999.49/DEN-74) | Priority-1 deep-dive below: semver crate choice, tag-reachability plumbing (empirically verified), conventional-commit classification, code placement, preflight-timing gap |
| 25d | A stalled run recovers without `kill -9` (999.44/DEN-68) | Priority-2 deep-dive below: PID discovery mechanism, TERM→KILL Rust idiom, wrapper/child reaping |
| 25e | Dead predicate with flaky tests stops feeding the 3-strike gate (999.47/DEN-72) | Priority-3 brief below: `#[deprecated]` + `-D warnings` interaction, concrete retarget path (an adjacent deterministic test already exists) |
| 25f | CONTRIBUTING.md release-procedure drift (no backlog entry) | Priority-3 brief below: both drift claims verified against live source |
| 999.38 | Test-suite PATH race (folded into 25b per D-14) | Priority-3 brief below: which call sites the `test_support`-style per-Command idiom transfers to cleanly, and which don't |
</phase_requirements>

## Summary

This phase is six independently-scoped, independently-evidenced fixes, not a new subsystem — nearly everything needed is already read-and-verify work against source that already exists, not new design. The one genuine design surface is 25c (versioning), and it is larger than its ROADMAP sizing states: what reads as "filter `count_git_tags` to semver tags" is actually a full replacement of `compute_version`'s three inputs (tag-count minor, `git describe`-distance patch, `Cargo.toml`-read major) with a new algorithm (reachable-tag baseline + conventional-commit classification + a preflight gate with its own state machine integration), a git-tag-reachability precedent already exists in this codebase for a closely related purpose, and one previously-undocumented consumer (`pipeline_gate.rs`'s finalization-gate test fixture) independently re-derives the *old* algorithm and will silently start asserting the wrong thing if left unrewritten.

25b, 25e, and 25f are as small as the ROADMAP says once the exact call sites are confirmed against live source (all three confirmed in this session). 25d is genuinely open on its root cause (why the monitor wrapper's `trap` isn't clearing a `SIGTERM`-ignoring backgrounded process) but the *mechanism* to build around that unknown — reuse the existing `(pid, starttime)` identity primitives, escalate to `SIGKILL` with a verified wait, and discover both process layers independent of any per-project registry — is well-supported by code already in this repository. 999.38's fix direction transfers cleanly to git-subprocess call sites but does **not** transfer cleanly to the `ensure_agent_binary`/`PATH`-scan call sites named in the ROADMAP; this document flags that gap explicitly rather than assuming the stated idiom applies uniformly.

**Primary recommendation:** Treat 25c as its own sub-track inside the phase — new dependencies (`semver`, optionally `git-conventional`), a rewritten `version.rs`, a new preflight check in `preflight.rs`, and a rewritten test fixture in `pipeline_gate.rs` — sized closer to M than the ROADMAP's folded-in S. Everything else in this document supports the ROADMAP's existing S/S–M sizes.

## Architectural Responsibility Map

DevFlow is a single-tier CLI (no browser/frontend-server/CDN split) — the relevant "tiers" here are internal module boundaries within the one binary + library workspace.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Base-ref currency check (25a) | `devflow-cli` (`commands.rs::start`, `preflight.rs`) | `devflow-core` (`git.rs` — reusable ancestor-status primitive) | Git-plumbing decision belongs next to the existing `origin_main_ancestor_status` precedent in `devflow-core`; the refusal/fetch policy decision belongs in the CLI's `start` orchestration |
| Staleness re-check scoping (25b) | `devflow-cli` (`commands.rs`, `pipeline_launch.rs`) | — | Pure call-site relocation within the CLI crate; no library API change |
| Version derivation (25c) | `devflow-core` (`version.rs`) | `devflow-cli` (`preflight.rs` — the major-bump gate) | `version.rs` already owns all git-derived version computation (library concern, no CLI state); the *gating decision* on a major bump is a pipeline-orchestration concern that belongs in the CLI's preflight machinery, matching D-09 |
| Orphan process discovery + reaping (25d) | `devflow-core` (`agent.rs` — process primitives) | `devflow-cli` (`commands.rs`/`doctor`/`gate sweep` — orchestration) | Low-level `/proc` + `libc::kill` primitives already live in `devflow-core::agent`; a new registry-independent scan is a natural sibling there. The *policy* of where it surfaces (doctor finding vs. new verb vs. `gate sweep` flag) is a CLI-orchestration decision |
| Dead-predicate cleanup (25e) | `devflow-core` (`agent.rs`) | `devflow-cli` (`commands.rs` tests) | The function and its tests are already split this way; no relocation needed |
| Docs drift (25f, 999.38) | Documentation / test-only | — | No production code changes; CONTRIBUTING.md and test-fixture-only |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `semver` | 1.0.28 (verified live via `cargo info`, published 2014, 14.5M weekly downloads, authored by `dtolnay` — the same author as Cargo's own reference SemVer implementation) [VERIFIED: crates.io registry via package-legitimacy seam] | Parse `MAJOR.MINOR.PATCH[-pre][+build]` strings and order them by real semver precedence (not string sort) | This is literally the crate Cargo itself uses for dependency-version resolution semantics; hand-rolling risks silently wrong ordering on any tag with a pre-release/build suffix, even though none exist in this repo's tag set today |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `git-conventional` | 1.1.0 (verified live via `cargo info`, published 2020, maintained by `crate-ci` — the org behind `cargo-release`/`typos`) [VERIFIED: crates.io registry via package-legitimacy seam] | Parse a commit's type/scope/`!`/description/body/footers per the Conventional Commits spec, including the `BREAKING CHANGE:` footer token | Recommended over hand-rolled regex for D-08's classifier — see Pattern discussion below for the specific footer/`!` edge cases a hand-roll would need to get right independently |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `semver` crate | Hand-rolled `(u64, u64, u64)` split-on-`.`-and-parse | Works for every tag in THIS repo today (none carry `-pre`/`+build` suffixes — verified: all 11 semver-shaped tags are plain `vMAJOR.MINOR.PATCH`), but silently mis-orders (or panics on unwrap) the day a tag like `v2.1.0-rc.1` appears, and duplicates logic Cargo's own dependency resolver already gets right. Given the crate is a single well-established dependency with zero transitive expansion of concern (MIT/Apache-2.0, `dtolnay`-maintained), recommend the crate. |
| `git-conventional` crate | Hand-rolled regex on `subject`/`body` | A hand-roll must independently get right: `!` placement (`feat(scope)!:` — after the optional scope, before the colon, not at the very start), the footer token comes in TWO spellings per spec (`BREAKING CHANGE:` and `BREAKING-CHANGE:`), and footers are multi-line and appear after a blank line separating them from the body. `git-conventional`'s `Commit::parse` returns a typed `Err` on malformed input, which maps directly onto D-10's "unrecognised → patch" floor (`Err` → patch) without the planner having to define what "malformed" means. Recommend the crate; it is a small, focused, single-purpose dependency with no transitive surprise (verified via `cargo info`: no heavy dependency tree). If the planner prefers zero new deps for this specific piece, a hand-roll IS tractable given the measured 118/120 conformance and the narrow rule set (type, optional `(scope)`, optional `!`, then look for a `BREAKING CHANGE:`/`BREAKING-CHANGE:` line in the footer) — but the two footer spellings and the blank-line-separated-footer-vs-body distinction are the two places a hand-roll is most likely to silently misclassify. |

**Installation:**
```bash
# in crates/devflow-core/Cargo.toml [dependencies]
cargo add semver@1
cargo add git-conventional@1   # optional — see Alternatives Considered
```

**Version verification:** Both versions above were confirmed live via `cargo info <pkg>` against the crates.io registry on 2026-07-27 (not from training-data memory) — see the Package Legitimacy Audit below for full signal detail.

## Package Legitimacy Audit

Ran `gsd-tools query package-legitimacy check --ecosystem crates semver git-conventional` (crates.io-backed):

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `semver` | crates.io | published 2014-11-11 (~11 yrs) | 14,525,147/wk | `github.com/dtolnay/semver` | OK | Approved |
| `git-conventional` | crates.io | published 2020-05-06 (~6 yrs) | 8,770/wk | `github.com/crate-ci/git-conventional` | OK | Approved |

**Packages removed due to `[SLOP]` verdict:** none.
**Packages flagged as suspicious `[SUS]`:** none.

Both packages were discovered via direct authoritative lookup (`cargo search`/`cargo info` against the live crates.io index, then cross-checked through the package-legitimacy seam) in this session, not from training-data recall — tagged `[VERIFIED: crates.io registry]` throughout this document, not `[ASSUMED]`.

**Dependency-budget note (relevant to CONTEXT.md's Claude's-Discretion item on this topic):** `devflow-core`'s current runtime dependency list is `libc`, `serde`, `serde_json`, `toml`, `thiserror`, `tracing` (6 deps) [VERIFIED: `crates/devflow-core/Cargo.toml`]. Neither `semver` nor `git-conventional` performs any network I/O — both are pure in-memory parsers — so PROJECT.md's "zero network deps" constraint (which the codebase's own dependency list confirms means *runtime* network calls, not build-time crate additions — there is no HTTP/TLS crate anywhere in the workspace) is not implicated. Note `semver` already appears in `Cargo.lock` today, but only as a *transitive* dependency of `wasmparser`/`wit-parser` [VERIFIED: `Cargo.lock` inspection] — it is not currently usable by `devflow-core` without an explicit `Cargo.toml` addition.

## Architecture Patterns

### System Architecture Diagram (25c's new version-derivation path)

```
devflow ship (Ship stage agent exits, outcome = Success)
        │
        ▼
handle_ship_outcome (pipeline_outcomes.rs)
        │
        ▼
run_preflight (preflight.rs) ── gated: stage == Ship (NEW: major-bump check joins
        │                        the existing generic_preflight_checks() list,
        │                        alongside preflight_interactivity_check /
        │                        preflight_gh_auth_check)
        │
        ├─ baseline = highest reachable semver tag ◄── git tag --merged HEAD (git.rs, NEW)
        │                                              │
        │                                              ▼
        │                                         filter: parses as semver (semver crate)
        │                                              │
        │                                              ▼
        │                                         max() by semver ordering
        │
        ├─ commits = git log --no-merges baseline..HEAD (worktree HEAD)
        │
        ├─ classify each commit (git-conventional or hand-roll) ─► highest-precedence bump
        │
        ├─ bump == Major? ──yes──► run_gate() [never-silent, D-09] ──► human Advance/Abort
        │        │
        │        no
        │        ▼
        │  preflight passes → launch_stage_inner spawns Ship-stage agent (/gsd-code-review, /gsd-ship)
        │        │
        │        ▼
        │  (COMMIT-RANGE TIMING GAP — see Pitfall 1 below: any commits the Ship
        │   agent itself makes here are NOT covered by the classification above)
        │        │
        │        ▼
        │  hooks_after_ship(): Merge → VersionBump → ChangelogAppend → BranchCleanup
        │        │
        │        ▼
        │  VersionBump calls version::compute_version() a SECOND time (NEW algorithm,
        │  same baseline+classify logic) — this is the version actually written & tagged
        ▼
     v{major}.{minor}.{patch} written to Cargo.toml, tagged, changelog updated
```

### Recommended Project Structure

No new files are required; the recommended placement keeps everything inside existing modules:

```
crates/devflow-core/src/
├── version.rs         # REWRITTEN: baseline-tag resolution, semver filter/ordering,
│                       #   conventional-commit classification, compute_version() rewired
│                       #   to the new algorithm. read_version/write_version UNCHANGED.
crates/devflow-cli/src/
├── preflight.rs        # NEW: a `preflight_major_bump_check` alongside the existing
│                       #   `preflight_interactivity_check`/`preflight_gh_auth_check`,
│                       #   wired into `generic_preflight_checks` gated on Stage::Ship
├── commands.rs          # 25b: enforce_build_staleness call hoisted here (before line 236)
├── pipeline_gate.rs     # 25c: `finalization_retry_gate_never_auto_approves_...` fixture
│                       #   MUST be rewritten — see Pitfall 2 below
```

### Pattern 1: Tag reachability via `git tag --merged`, not per-tag `--is-ancestor` loops

**What:** `git tag --merged HEAD` lists exactly the tags whose target commit is an ancestor of `HEAD`, in ONE subprocess spawn, rather than spawning `git merge-base --is-ancestor <tag> HEAD` once per tag (O(n) spawns).

**When to use:** D-07's reachability filter.

**Verified empirically against this repository (12 tags, 2026-07-27):**
```bash
$ git tag --merged HEAD | sort -V
v1.0.1
v1.2.0
v1.3.0
v1.3.69
v1.4.0
v1.5.0
v1.6.0
v1.7.0
v1.8.0
v1.8.1
v2.0.0
# archive-planning-docs-2026-07-24 is absent — matches per-tag --is-ancestor exactly
```
This is byte-identical to looping `git merge-base --is-ancestor <tag> HEAD` over all 12 tags (verified both ways, same session) [VERIFIED: live git output, this repository, 2026-07-27].

**Annotated vs. lightweight tags — no difference for this purpose, verified not assumed.** `v1.3.69` is a **lightweight** tag (`git cat-file -t v1.3.69` → `commit`, not `tag`); every other semver-shaped tag in this repo is annotated (`git cat-file -t` → `tag`). Both forms appear identically in the `--merged` output and both dereference correctly for `--is-ancestor` — git's `--merged` machinery dereferences a tag object to its target commit before comparing, same as `--is-ancestor` does. No special-casing is needed for lightweight tags.

**Recommendation:** use `git tag --merged HEAD`, not a per-tag loop. This directly parallels an existing precedent already in this codebase — `GitFlow::merged_branches` (`git.rs:243`) uses `git branch --merged` for the analogous "which branches are already reachable from develop" question, rather than looping `--is-ancestor` per branch. The planner should follow that same shape for tags.

**Example:**
```rust
// New helper in version.rs, following the existing `count_git_tags` idiom
// (Command::new("git"), .current_dir(project_root), parse stdout lines):
fn reachable_semver_tags(project_root: &Path) -> Result<Vec<semver::Version>, VersionError> {
    let output = Command::new("git")
        .args(["tag", "--merged", "HEAD"])
        .current_dir(project_root)
        .output()
        .map_err(|err| VersionError::Git(err.to_string()))?;
    if !output.status.success() {
        return Err(VersionError::Git(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|tag| tag.trim().strip_prefix('v').or(Some(tag.trim())))
        .filter_map(|stripped| semver::Version::parse(stripped).ok())
        .collect())
}
```
Note the `strip_prefix('v')` — the `semver` crate does **not** accept a leading `v` [VERIFIED: `semver` crate's documented grammar is bare `MAJOR.MINOR.PATCH`, no prefix]; every tag in this repo is `v`-prefixed, so this strip is required, not optional.

### Pattern 2: `--merged`'s "not reachable" case, and D-10's refuse behavior

If the true highest tag by string/creation order is NOT in the `--merged` output at all (the D-12 sync-discipline coupling — a squashed `develop`→`main` sync breaks ancestry), D-10 requires refusing loudly rather than silently using the highest *reachable* tag. This means the algorithm needs to compute the highest tag **before** filtering by reachability (a second `git tag` call, or reuse of `count_git_tags`'s existing raw enumeration filtered to semver-parseable-only, without the `--merged` restriction) so it has something to name in the refusal message. This is a second, cheap `git tag` call (already an existing idiom in `version.rs::count_git_tags`) — no new plumbing needed, just don't discard the raw list before computing both the reachable-max and the overall-max.

### Pattern 3: Conventional-commit format string for footer detection

**What:** `git log --no-merges --format='<sep>' baseline..HEAD` must capture subject AND body/footer separately, because `BREAKING CHANGE:` is a footer token, not something that ever appears in the subject line.

**Recommendation:** Use a format string with an unlikely-to-collide field separator (git's own `%x1f`/`%x1e` unit/record-separator idiom is the standard safe choice, since commit messages can contain arbitrary characters including the pipe/comma characters a naive delimiter would pick):
```rust
Command::new("git")
    .args([
        "log", "--no-merges",
        &format!("{baseline}..HEAD"),
        "--format=%H%x1f%B%x1e",  // %B = raw body (subject + blank line + full body/footers)
    ])
```
`%B` (raw, unwrapped body) is preferable to separately requesting `%s` (subject) and `%b` (body) — `git-conventional::Commit::parse` (if used) expects the FULL raw commit message text (subject line, blank line, then body/footers) as its input, matching git's own on-disk format, so `%B` requires no reassembly. If hand-rolling instead of using the crate, split on the first `\n\n` to separate subject from body/footers.

**`!` detection (hand-roll path only):** the breaking marker sits after an optional `(scope)` and before the colon: `type(scope)!: subject` or `type!: subject`. A hand-rolled check should match the regex-equivalent `^\w+(\([^)]*\))?!:` against the subject line — NOT a bare "does the subject contain `!`" (which would false-positive on any subject containing `!`, e.g. "fix: don't panic!").

### Pattern 4: `run_preflight`'s existing shape — what a new check plugs into

**What:** `run_preflight` (`preflight.rs:381`) already composes `generic_preflight_checks` (currently `preflight_interactivity_check` + `preflight_gh_auth_check`, both `fn(...) -> Result<(), String>`) with an adapter-specific hook, and on any `Err(reason)`:
1. checks `state.preflight_retries >= mode::MAX_PREFLIGHT_RETRIES` (abort if so, emitting `preflight_retry_ceiling_reached`)
2. otherwise increments `preflight_retries`, persists, and calls `run_gate(project_root, state, stage, &context)` with a `"[never-silent] preflight failed for stage {stage}: {reason}"` message
3. dispatches on the returned `GateAction` (`Advance` → skip the re-check, relaunch via `launch_stage_inner`; `LoopBack` → full `launch_stage` re-entry, bounded by the same ceiling; `Abort` → `abort()`)

**Recommendation for D-09:** add a new `fn preflight_major_bump_check(project_root: &Path, state: &State) -> Result<(), String>` mirroring `preflight_gh_auth_check`'s shape exactly — gated on `stage == Stage::Ship` the same way `gh_auth_check_applies` gates on Ship, added to `generic_preflight_checks`'s composition chain (`preflight_interactivity_check(...).and_then(|()| preflight_major_bump_check(...))` or equivalent). This gives the check the SAME gate+notify+ceiling machinery for free, and the SAME `Advance`-skips-recheck semantics D-18f already established (a human `Advance` on a major-bump gate does not re-run the classification, matching `preflight_gh_auth_check`'s deterministic-idempotent-check assumption documented at `preflight.rs:366-375`).

**Verdict type:** the existing checks return `Result<(), String>` — a bare error string, not a rich enum. For the major-bump check, the `Err(reason)` string should name the computed bump kind and the exact commits/tags involved (mirroring `unreachable_message`'s style of a self-contained, actionable message with no absolute paths — see the WR-02 no-path-leak precedent at `commands.rs`'s `events::emit` comment).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Semver parsing/ordering | A `(u64,u64,u64)` split-and-compare | `semver` crate | This repo's tags happen to have no pre-release/build suffixes today, but the crate is Cargo's own reference implementation, MIT/Apache-2.0, zero-risk to add, and removes an entire class of future "why did v2.0.0-rc.1 sort after v2.0.0" bugs |
| Conventional-commit footer/breaking-marker parsing | Regex against raw commit text | `git-conventional` crate (or, if the planner prefers zero new deps for this piece specifically, a carefully-scoped hand-roll — see Alternatives Considered above for the exact two footer-spelling and blank-line-separation traps to get right) | The spec has two spellings of the breaking-change footer token and specific whitespace/blank-line rules around where a footer begins; a naive regex is the likeliest place D-08's classifier silently misclassifies |
| Process liveness / signalling | A hand-rolled `/proc` PID scan and `kill` via shelling to the `kill(1)` binary | `agent::agent_running`, `agent::terminate`, `agent::process_start_time`, `agent::is_same_process` (all already exist in `devflow-core::agent`) | These already handle the zombie trap, the pid-0/negative-pid `kill()` group-signalling hazard, and the `i32::MAX` wraparound — re-deriving any of this for 25d risks reintroducing a bug this codebase already fixed once |

**Key insight:** every "don't hand-roll" item above already has either a battle-tested crate one `cargo add` away, or a battle-tested in-repo primitive one function-call away — 25c and 25d are compositions of existing tools, not new low-level engineering.

## Common Pitfalls

### Pitfall 1: The Ship-stage preflight check evaluates a commit range that is not necessarily final

**What goes wrong:** D-09 places the major-bump classification inside `run_preflight`, which runs **before** `launch_stage_inner` spawns the Ship stage's agent (`pipeline_launch.rs:169-199`). The Ship stage's own agent runs `/gsd-code-review` (writes `REVIEW.md`) and then, if no Critical findings, `/gsd-ship` (create PR, run review, prepare for merge) [VERIFIED: `crates/devflow-core/src/prompt.rs::ship_stage_prompt`]. **This session could not fully confirm whether `/gsd-ship`'s own execution ever commits to the feature branch** (its GSD skill delegates to an external, globally-versioned workflow file outside this repository's own source, `$HOME/.claude/gsd-core/workflows/ship.md`, not inspected in depth here) [ASSUMED — flagged, not verified]. If it does — even a single late `fix:`/`feat:`/breaking commit made during Ship's own review-and-fix step — the major-bump classification that already passed at preflight time will never see it, because `hooks_after_ship`'s `Merge` step merges whatever is on the feature branch **at Merge time**, which is strictly later than the preflight check.

**Why it happens:** `run_preflight` is structurally a stage-*launch* gate, not a pre-*merge* gate. D-09 explicitly locks the placement as "inside `run_preflight`" for a correct reason (must run before `hooks_after_ship`, which has no rollback) — but "before `hooks_after_ship`" is satisfied by BOTH "before the Ship agent spawns" (what `run_preflight` actually gives) AND "after the Ship agent exits, before `Merge` runs" (a tighter, not-yet-existing checkpoint). D-09 picked the looser of the two without the pipeline-timing detail being visible at discussion time.

**How to avoid:** Report this to the planner plainly (per the discuss-phase scope note's explicit instruction — this is exactly the answer to "if the commit range is not reliably available at preflight, say so plainly"). Two honest options, neither of which this document decides for the operator:
1. **Accept the gap as documented residual risk.** D-10's "unrecognised → patch" floor and D-06's whole-classifier design already assume most Ship-stage activity is non-breaking; a late Ship-stage commit that happens to be a genuine breaking change would silently under-classify. Low probability (Ship-stage review/fix commits are typically small, and `/gsd-code-review`'s own Critical-severity gate already stops before `/gsd-ship` runs if something serious was found), but not zero.
2. **Add a second, defense-in-depth classification immediately before the `Merge` hook fires** (inside `handle_ship_outcome`/wherever `hooks_after_ship` is invoked, not inside `run_preflight`), re-running the SAME classification function against the (now truly final) commit range, and gating on that result too. This satisfies D-09's "before `hooks_after_ship` runs at all" language just as literally as the preflight placement does, closes the gap completely, and reuses the same classification code — but is an additional integration point D-09's text did not explicitly call out, so the planner should treat this as a recommendation to confirm with the operator, not a silent scope addition.

**Warning signs during execution:** if a plan implements ONLY the preflight-time check and the Ship-stage agent's actual commit behavior during `/gsd-ship` is never verified, this gap ships unexamined.

### Pitfall 2: `pipeline_gate.rs`'s existing test fixture independently re-derives the OLD version algorithm

**What goes wrong:** `finalization_retry_gate_never_auto_approves_even_with_yes_ship_set` (`pipeline_gate.rs:809-840`) precomputes the exact tag `VersionBump` will attempt to create — using `version::compute_version(root).major` and `version::count_git_tags(root)`, then builds `v{major}.{tags_before+1}.0` and pre-creates that tag to force `VersionBump`'s `git.tag()` call to collide (the test's whole point is to exercise the finalization-retry gate when `VersionBump` fails). **This is not mentioned anywhere in CONTEXT.md's "Integration Points" list** (which names only `hooks.rs` and `version.rs`'s own internals as `compute_version`'s consumers) — it is a previously-undocumented call site discovered in this research session.

**Why it happens:** `count_git_tags` is currently a raw, unfiltered tag count (D-07 replaces it with a reachable-semver-tag lookup), so this fixture's "predict the next tag" logic is baked into the OLD minor-from-tag-count scheme. Once D-07/D-08 land, `compute_version`'s minor/patch no longer come from `(tag count, describe distance)` — they come from `(highest reachable semver baseline, conventional-commit classification of baseline..HEAD)`. The fixture's arithmetic (`tags_before + 1` as the next minor) has no equivalent in the new scheme.

**How to avoid:** This fixture MUST be rewritten as part of 25c, not left as an incidental casualty discovered by a failing test during 25c's own implementation. Since the fixture repo's commits between its fabricated baseline and `HEAD` are just fixture setup (branch creation, no real feature commits), the new algorithm's D-10 floor ("nothing bumps → patch anyway") is almost certainly what determines the fixture's expected next tag — the rewrite should compute the SAME way `version_bump` now does (call the real, new `compute_version`), not hand-derive an independent prediction that could drift from the implementation a second time.

**Warning signs:** `cargo test --workspace` failing in `pipeline_gate.rs` after 25c's `version.rs` changes land, with a tag-collision assertion failure — this is expected and confirms the coupling; it is not a new regression to chase.

### Pitfall 3: 999.38's "per-Command env" idiom does not transfer uniformly to the named call sites

**What goes wrong:** The ROADMAP's fix direction for 999.38 ("per-`Command` `env`/`env_remove`, as `test_support::git_command` now does for git") is correct for git-*subprocess*-based call sites, but at least one of the five named locations (`pipeline_launch.rs:590`, `pipeline_outcomes.rs:879/1132/1246`, `preflight.rs:627/701`) neutralizes `PATH` to control `ensure_agent_binary`'s outcome, and `ensure_agent_binary`/`agent_binary_available` (`preflight.rs:61-91`) is **not** `Command`-based — it does a manual `std::env::var_os("PATH")` + `std::env::split_paths` scan with no subprocess spawn at all. There is no `Command` to attach `.env()`/`.env_remove()` to for that specific check.

**Why it happens:** 999.37's `test_support::git_command`/`hermetic_command` idiom works because every git invocation in this codebase already goes through `std::process::Command`, so scrubbing per-`Command` env is a drop-in replacement for scrubbing process-global env. `ensure_agent_binary` predates that pattern and reads the environment directly, not via a `Command` builder.

**How to avoid:** For the git-`Command`-based portions of the affected tests (any place a test also drives `monitor::spawn_monitor`, whose `Command::new("sh")` already accepts `.envs(...)` — see `monitor.rs:156`), the per-Command idiom applies cleanly: pass an explicit, scoped `PATH` override to just that `Command`, leaving ambient process env untouched. For the `ensure_agent_binary`/`agent_binary_available` pre-check itself, the clean fix is a small signature change — thread an explicit search-path value (or a closure/trait for "resolve this env var") into `agent_binary_available` instead of reading `std::env::var_os` directly, so tests can inject a value with **zero** process-global mutation, rather than trying to force the existing idiom onto a function that isn't `Command`-shaped. Note also that `preflight.rs:627/701`'s existing helper `test_support::prepend_path` (`test_support.rs:263-272`) already preserves the REST of the real `PATH` when stubbing an agent binary — so the git-spawn race those two specific tests are exposed to is about the inherent unsoundness of concurrent `std::env::set_var`/reads (Rust 2024 marks these `unsafe` for exactly this reason — the hazard is a torn/racing read of the process environment table itself, not merely "the wrong PATH value"), not about `git` actually being absent from the stubbed PATH.

**Warning signs:** a plan that mechanically converts all five call sites to `.env()`-on-a-`Command` without first confirming which of them are actually `Command`-based will either not compile (no `Command` to attach to) or silently fail to fix the race for the non-`Command` call site(s).

### Pitfall 4: `libc` is already a dependency — do not add `nix` for 25d

**What goes wrong:** A natural instinct for "send SIGTERM, wait, escalate to SIGKILL" in Rust is to reach for the `nix` crate, which wraps these syscalls more ergonomically than raw `libc`.

**Why it happens:** `nix` is indeed the more idiomatic choice for greenfield Unix-process code.

**How to avoid:** This codebase already has `libc = "0.2"` as a direct `devflow-core` dependency [VERIFIED: `crates/devflow-core/Cargo.toml`], already uses it for `libc::kill(pid, 0)` (liveness) and `libc::kill(pid, libc::SIGTERM)` (`agent::terminate`), and `libc::SIGKILL` is available from the same crate with zero additional dependency cost. Adding `nix` on top would be a second, overlapping process-control dependency for no behavioral gain — extend `agent.rs` with the escalation logic using the same `libc` primitives already in place (e.g. a new `agent::terminate_and_verify` or an escalation loop built from the existing `terminate`/`agent_running` pair), not a new crate.

**Warning signs:** a `Cargo.toml` diff for 25d that adds `nix` — this should prompt a second look; the existing `agent.rs` primitives (`terminate`, `agent_running`, `process_start_time`, `is_same_process`) are the correct foundation.

## Runtime State Inventory

Not applicable — this is a bug-fix phase with no rename/rebrand/refactor/string-replacement/migration character. No entity is being renamed; no persisted-data schema changes (D-03 explicitly avoids adding a new `State` field; D-11 changes what `compute_version` *reads*, not any on-disk schema shape — `write_version`'s file format is unchanged). Skipping this section per its own trigger condition.

## Code Examples

### Existing precedent: `AncestorStatus` + `origin_main_ancestor_status` (25a's direct template)

```rust
// Source: crates/devflow-core/src/git.rs:462-508 (existing, live code — 20d / `devflow release --check`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AncestorStatus {
    Ancestor,
    Diverged,
    RefAbsent,
}

pub fn origin_main_ancestor_status(project_root: &Path) -> AncestorStatus {
    let ref_exists = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "origin/main"])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !ref_exists {
        return AncestorStatus::RefAbsent;
    }
    let is_ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", "origin/main", "HEAD"])
        .current_dir(project_root)
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if is_ancestor { AncestorStatus::Ancestor } else { AncestorStatus::Diverged }
}
```
This is issued with **no `git fetch`** (against already-fetched local refs), and distinguishes "the remote-tracking ref doesn't even resolve locally" (`RefAbsent`) from "it resolves but has diverged" (`Diverged`) — exactly the three-way distinction 999.51's option 3 (compare-and-refuse-loudly) needs, just pointed at `origin/develop`/`develop` instead of `origin/main`/`HEAD`. This is the concrete "established in-repo pattern" the discuss-phase scope note asked this research to locate.

### Existing precedent: `enforce_build_staleness`'s exact signature and the D-03 hoist's verified precondition

```rust
// Source: crates/devflow-cli/src/staleness.rs:329-334 (existing, live code)
pub(crate) fn enforce_build_staleness(
    project_root: &Path,
    state: &State,
    embedded_commit: &str,
    build_dirty: bool,
) -> Result<(), CliError>
```
Called today from `launch_stage_inner` (`pipeline_launch.rs:93-98`) as:
```rust
enforce_build_staleness(
    &project_root,
    state,
    env!("DEVFLOW_BUILD_COMMIT"),
    env!("DEVFLOW_BUILD_DIRTY") == "true",
)?;
```
**Verified live**: `commands.rs`'s `start` function already has `use crate::staleness::run_git_stdout;` at the top of the file (`commands.rs:21`), confirming `staleness` is already an in-scope module for `commands.rs`, and `enforce_build_staleness` is `pub(crate)` (crate-visible), so calling it from `commands.rs` requires only importing the function name — no visibility change needed. `state.worktree_path` is set at `commands.rs:199`; `launch_stage(&mut state, None, None)` is called at `commands.rs:236`. D-03's hoist point (between 199 and 236) is confirmed correct and requires no reordering of surrounding logic — `workflow::save_state` and the `workflow_started` event emission both currently sit between those two lines and are unaffected by inserting the staleness call anywhere in that span, as long as it runs after `state.worktree_path` is set.

### Existing precedent: identity-pair reaping in `stop_via_lock` (25d's foundation)

```rust
// Source: crates/devflow-cli/src/commands.rs:1191-1223 (existing, live code — abbreviated)
match lock::holder_identity(project_root, phase) {
    Some((recorded_pid, Some(recorded_start))) if recorded_pid == pid => {
        if !agent::is_same_process(pid, recorded_start) {
            return Err(/* refuse: pid recycled */);
        }
    }
    Some((_, None)) => return Err(/* refuse: legacy lock, no start time recorded */),
    _ => return Err(/* refuse: could not read back identity */),
}
if agent::terminate(pid) {
    println!("stop: signalled pid {pid}, phase {phase}'s lock holder");
}
```
Note `terminate` (`agent.rs:75-80`) sends **exactly one** `SIGTERM` with no escalation and no verification of death — this is the gap 25d must close. The existing identity-matching logic above is exactly what 999.44's "prefer reusing that identity mechanism over inventing a second one" (CONTEXT.md) is pointing at; the new work is (a) an escalation loop after `terminate` returns, and (b) a registry-independent discovery path for processes that have no lock file at all (see Pitfall discussion below on why state-orphaned processes are literally invisible to file-based discovery).

### The monitor wrapper's exact shell script (25d's process-tree shape)

```bash
# Source: crates/devflow-core/src/monitor.rs:134-146 (existing, live code, reconstructed from the format! template)
apid=''; cleanup() { [ -n "$apid" ] && kill "$apid" 2>/dev/null; exit 0; }
trap cleanup TERM INT
cd '<workdir>' || exit 1
"$@" > '<stdout_file>' 2>'<stderr_file>' &
apid=$!; echo $apid > '<pid_file>'
wait $apid; echo $? > '<exit_file>'; '<devflow-binary>' advance '<project_root>' --phase <N>
```
This is spawned via `Command::new("sh").arg("-c").arg(&script).arg("sh").arg(program).args(args)` — `"$@"` inside the script therefore expands to `program args...` (the agent), NOT including the leading `"sh"` (which becomes `$0`/the script's own name, excluded from `"$@"`).

**The two-layer process tree, confirmed from source (not just from the ROADMAP's field observation):**
1. **Layer 1 — the wrapper `sh` process itself.** This is what `state.monitor_pid` records (`pipeline_launch.rs:126-131`). It runs the whole script above.
2. **Layer 2 — whatever the wrapper currently owns as a child.** During the agent's run, this is the backgrounded agent (`$apid`, recorded in the separate `agent_pid_path` file, readable via `monitor::wait_for_agent_pid`). AFTER `wait $apid` returns (the agent has exited) and the script proceeds to its final `;`-separated command, this is instead the newly-forked `devflow advance ...` child process — a *different* pid than `$apid`, and one that (per this session's source reading) is recorded nowhere on disk at all. This matches the ROADMAP's 999.44 finding precisely: "killing the wrappers then orphaned their `devflow advance` children" — those children are exactly this un-recorded second layer.

**Why registry-based discovery cannot see either layer once its root is gone:** `registry::prune_missing()` (called at the top of `gate_sweep`, `commands.rs:1000`) removes a project root's registry entry the moment the root directory no longer exists on disk. For the e2e-test-fixture orphans this backlog item's evidence comes from (`/tmp/.tmp*` scratch roots, per 999.46), once the scratch directory is deleted by test teardown, `.devflow/lock-{phase:02}` under that root is gone too — so `lock::holder`/`lock::holder_identity` (both file-reads under `project_root`) return `None` even while the OS process is still alive. **This is why 999.44's fix direction is explicitly "registry-independent"**: file-based discovery (lock files, registry entries, state files) is structurally unable to find a process whose root directory has been deleted out from under it. The only remaining discovery surface is the OS process table itself.

**Recommended discovery approach (composable from what's confirmed above):** scan `/proc/*/cmdline` for processes whose argv matches either shape precisely — Layer 1: `sh -c <script containing the literal, DevFlow-specific marker "trap cleanup TERM INT">`, or Layer 2 post-agent-exit: argv `[0]` ends in `devflow` (or the known binary name) AND argv`[1] == "advance"`. Matching on a literal, multi-token script fragment (Layer 1) or on `argv[0]`+`argv[1]` together (Layer 2) is deliberately **narrower** than 999.47's disproven `looks_like_devflow_process` approach (which matched ANY argv element whose basename starts with `devflow`, including a path merely mentioned as an argument) — this avoids reintroducing that same false-positive class while still not depending on any per-project registry or lock file. Do not require `ppid == 1` anywhere in the discovery logic — `systemd --user` (or an equivalent per-host subreaper) is what actually adopts these orphans, and a `ppid == 1` filter was directly measured (23-FINDINGS.md, this repository) to report **zero** orphans while 14 genuinely existed.

### The escalation-with-verification gap `agent::terminate` needs closed

```rust
// Recommended shape, built from existing agent.rs primitives — not yet in source
pub fn terminate_and_verify(pid: u32, wait: std::time::Duration, poll: std::time::Duration) -> bool {
    if !terminate(pid) {          // existing: SIGTERM
        return !agent_running(pid); // already dead — success
    }
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if !agent_running(pid) {  // existing: kill(pid, 0) + zombie check
            return true;
        }
        std::thread::sleep(poll);
    }
    // Escalate: SIGKILL, using the same libc already in this crate.
    let Ok(signed) = libc::pid_t::try_from(pid) else { return false };
    if signed > 0 { unsafe { libc::kill(signed, libc::SIGKILL) }; }
    !agent_running(pid)  // verify death — never assume it
}
```
This reuses `terminate`/`agent_running` verbatim and adds only the bounded-wait-then-`SIGKILL`-then-verify shape CONTEXT.md's D-17 requires ("must escalate `TERM`→`KILL` with a bounded wait and *verify* death rather than assume it"). The regression test CONTEXT.md asks for ("a `TERM`-ignoring child is still cleared") is directly testable by spawning a process that traps and ignores `SIGTERM` (e.g. `sh -c 'trap "" TERM; sleep 30'`) and asserting `terminate_and_verify` still returns `true`.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| `compute_version`: major from `Cargo.toml`, minor from raw `git tag` line count, patch from `git describe --tags --abbrev=0` distance | (25c, this phase) major/minor/patch all derived from `(highest reachable semver tag, conventional-commit classification of the range since it)` | This phase | `Cargo.toml` stops being a version *input* entirely (D-11); the June-2026 commit-message-versioning ban (ROADMAP.md:36, PROJECT.md Constraints) is explicitly lifted (D-06) and both docs must be updated in-phase |
| `enforce_build_staleness` re-checked at every stage transition (`launch_stage`) | Checked once at `devflow start`/`devflow resume`'s initial launch only (25b) | This phase | A phase that modifies DevFlow's own source can now complete unattended; a *different* binary resuming a phase mid-run is no longer re-adjudicated (D-04, accepted trade) |
| `looks_like_devflow_process` as a signalling-authorization signal | `(pid, starttime)` identity pair (already shipped, pre-dates this phase — confirmed via CONTEXT.md D-02 and this session's source read) | Already shipped (999.47's production fix landed before this phase started) | 25e is retiring dead code and flaky tests that no longer reflect what production does, not hardening a live guard |

**Deprecated/outdated:**
- `count_git_tags`/`commits_since_last_minor_tag` (`version.rs:90-138`): both fully superseded by D-07's reachable-tag-baseline approach; no `git describe` call survives anywhere in the new algorithm.
- `looks_like_devflow_process` (`agent.rs:158-172`): marked `#[deprecated]` in this phase per D-13, retained (not deleted) until the next real breaking-change major.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The Ship-stage agent's `/gsd-ship` execution may add commits to the feature branch after the preflight-time major-bump classification runs, and this session could not fully confirm whether it does (its logic lives in an external, globally-versioned GSD workflow file outside this repository) | Common Pitfalls, Pitfall 1 | If it does NOT commit anything, D-09's preflight-only placement is fully sufficient and Pitfall 1's proposed defense-in-depth second check is unnecessary extra scope. If it DOES commit, a late breaking-change commit during Ship could ship unattended past the major-bump gate. The planner should verify this directly (read `$HOME/.claude/gsd-core/workflows/ship.md`, or check git history/event logs from a prior real Ship-stage run) before deciding whether to add the second check. |
| A2 | A hand-rolled conventional-commit classifier (as an alternative to the `git-conventional` crate) is described as "tractable" given 118/120 measured conformance, but this session did not attempt to hand-write and test one against this repository's actual commit history | Standard Stack, Alternatives Considered | If the planner chooses the hand-roll path, the two specific traps named (dual footer-token spelling, blank-line-separated footer detection) should be explicitly covered by unit tests, not assumed correct by inspection. |

## Open Questions

1. **Does `/gsd-ship`'s execution (the second half of the Ship-stage agent's work) ever create commits on the feature branch?**
   - What we know: `/gsd-code-review` (the first half) writes `REVIEW.md` and gates on Critical findings without proceeding to `/gsd-ship` if any exist; `/gsd-ship`'s own description is "push branch, create PR with auto-generated body, optionally trigger review, and track the merge" — none of which obviously requires a local commit, but "optionally trigger review" leaves room for a review-and-fix cycle that could.
   - What's unclear: the actual step-by-step behavior lives in `$HOME/.claude/gsd-core/workflows/ship.md`, a GSD-harness file outside this repository's own version control, not deeply inspected in this research pass.
   - Recommendation: the planner (or the executor, before finalizing 25c's plan) should read that workflow file directly, or instrument/observe one real Ship-stage run, before deciding whether Pitfall 1's defense-in-depth second check is in-scope for this phase or an accepted, documented residual risk.

2. **Why does the monitor wrapper's `trap cleanup TERM INT` not fire on a `SIGTERM`-ignoring or slow-to-die backgrounded process?**
   - What we know: the script structure is confirmed from source (see Code Examples above); the operator's working hypothesis is the shell being blocked in `wait $apid`; POSIX shells are generally specified to interrupt a blocking `wait` on signal receipt and run pending traps, so this hypothesis is plausible but not something this research session could reproduce or disprove (no live orphan population was available to probe in this session).
   - What's unclear: whether the actual mechanism is shell-specific (`dash` vs `bash` as `/bin/sh` on the target host), a signal-delivery timing issue specific to a backgrounded process that has itself become a process-group leader, or something else entirely.
   - Recommendation: per D-17/CONTEXT.md's own instruction, the fix does **not** need to depend on resolving this — `terminate_and_verify`'s escalate-to-SIGKILL-with-verification shape (see Code Examples) works regardless of why `SIGTERM` alone fails. Treat this as accepted unexplained behavior the code defends against, not a root cause the plan must fix at its source, matching this project's own stated lesson (999.47: "the fix does not depend on resolving that").

## Environment Availability

Not applicable in the tool/service sense — this phase's only new external dependencies are two crates.io libraries (`semver`, `git-conventional`), both already confirmed resolvable (see Package Legitimacy Audit). `git` itself is an existing, already-required runtime dependency (PROJECT.md Constraints: "Runtime: `git` required") with no version-floor concern raised by this phase — every git subcommand used (`tag --merged`, `log --format`, `merge-base --is-ancestor`) has been stable, documented git plumbing for well over a decade.

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | 25a, 25c (all units) | ✓ | (existing project requirement, unchanged) | — |
| `semver` crate | 25c | ✓ (verified via `cargo info`, not yet in `devflow-core`'s own `Cargo.toml`) | 1.0.28 | Hand-roll possible but not recommended — see Don't Hand-Roll |
| `git-conventional` crate | 25c (optional) | ✓ (verified via `cargo info`, not yet a dependency) | 1.1.0 | Hand-rolled classifier — see Alternatives Considered for exact traps to cover |
| `libc` crate | 25d | ✓ (already a direct `devflow-core` dependency) | 0.2 (already pinned in `Cargo.toml`) | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none blocking — both new crates have a documented, if less-recommended, hand-roll fallback.

## Validation Architecture

`.planning/config.json` has `workflow.nyquist_validation: true` (present and true) — this section is required, not skippable.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` harness via `cargo test` (no external test framework — `tempfile` is the only `dev-dependencies` addition in either crate) |
| Config file | none — `scripts/check.sh` is the project's canonical local/CI runner wrapper |
| Quick run command | `cargo test --workspace <module-path filter>` (e.g. `cargo test --workspace version::` or `cargo test --workspace staleness::`) |
| Full suite command | `scripts/check.sh` (= `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| 25a | Refuse (or fetch, per whichever option is chosen) when local `develop` is stale relative to `origin/develop` | unit | `cargo test --workspace start_reachability` (new tests alongside the existing `phase_reachability_on_base` suite in `preflight.rs`) | ❌ Wave 0 |
| 25b | `enforce_build_staleness` fires once at `start`/`resume`, not on every stage transition | unit | `cargo test --workspace enforce_build_staleness` (existing suite in `staleness.rs`/`pipeline_launch.rs`; needs an ADDED test asserting a mid-run stage transition does NOT re-invoke the check) | ✅ (existing suite), ❌ (new not-re-invoked assertion) Wave 0 |
| 25c | `compute_version` derives `v2.0.0`-shaped output from `(reachable baseline, classified bump)`, refuses on unreachable-highest-tag, floors on no-bump/unrecognised | unit | `cargo test --workspace version::` (full rewrite of the existing suite — see Pitfall 2) | ✅ (existing file, needs rewrite) |
| 25c | Major-bump preflight gate fires and never auto-ships | unit + integration | `cargo test --workspace preflight_major_bump` (new) | ❌ Wave 0 |
| 25c | `pipeline_gate.rs`'s finalization-retry fixture still predicts the correct next tag under the new algorithm | integration | `cargo test --workspace finalization_retry_gate_never_auto_approves` (existing, needs rewrite per Pitfall 2) | ✅ (existing, needs rewrite) |
| 25d | `terminate_and_verify` escalates TERM→KILL and clears a SIGTERM-ignoring child | unit | `cargo test --workspace terminate_and_verify` (new) | ❌ Wave 0 |
| 25d | Registry-independent discovery finds a process whose root directory no longer exists | integration | new test in `commands.rs` or a new module, spawning a real child under a since-deleted temp root | ❌ Wave 0 |
| 25e | Retargeted tests assert `(pid, starttime)` identity, not `looks_like_devflow_process` | unit | `cargo test --workspace looks_like_devflow_process_is_false_for_a_non_devflow_process` / `stop_refuses_to_signal_a_live_pid_that_fails_the_identity_check` (both existing, rewritten in place) | ✅ (existing, needs rewrite) |
| 999.38 | `ahead_build_from_descendant_commit_warns_instead_of_blocking` no longer races concurrent git-shelling tests | unit (flake-class) | `cargo test --workspace ahead_build_from_descendant_commit_warns_instead_of_blocking` (existing; flake reproduction requires a full-suite concurrent run, not this single-test invocation) | ✅ existing |
| 25f | CONTRIBUTING.md step 5 matches the actual signing procedure; ROADMAP's Phase 25 Acceptance paragraph matches D-15 | manual-only (docs) | N/A — doc review, no automated assertion | N/A |

### Sampling Rate

- **Per task commit:** targeted `cargo test --workspace <module>` filter matching the module just touched.
- **Per wave merge:** `scripts/check.sh` (full fmt+clippy+test chain — this is what CI itself runs, per `.github/workflows/ci.yml`).
- **Phase gate:** full `scripts/check.sh` green before `/gsd-verify-work`, PLUS — because 999.47's confirmed mechanism only reproduces reliably inside the pinned CI container (local runs did not reproduce the fork/exec race even at 4000 spawns) — the 25e retarget's success (no more flakes) can only be confirmed by observing CI-on-branch stability across a few pushes, not a single local green run. Note this explicitly in the plan's verification steps; local-green is insufficient for 25e specifically, matching this project's own established precedent (19-RESEARCH.md: "Verification must be CI-on-branch — local-green is explicitly insufficient").

### Wave 0 Gaps

- [ ] New test module/section for 25a's chosen option (whichever of the three the planner selects) — no existing test file covers base-ref currency at all.
- [ ] New tests for `preflight_major_bump_check` (25c) — no existing coverage of a major-bump preflight gate.
- [ ] New tests for `terminate_and_verify` (25d) — no existing escalation-with-verification coverage; `agent::terminate` today has only single-SIGTERM tests.
- [ ] New test(s) for registry-independent orphan discovery (25d) — no existing test spawns a child, deletes its root directory out from under it, and then asserts discovery still finds it.
- [ ] No framework install needed — `cargo test --workspace` already covers the whole workspace; no new test runner or config required.

## Security Domain

`security_enforcement` is not present in `.planning/config.json` — treated as enabled per the default.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | no | No authentication surface touched by this phase |
| V3 Session Management | no | Not applicable — no session/token concept in this CLI |
| V4 Access Control | yes (25d) | Process-signalling authorization — the existing `(pid, starttime)` identity-pair check (`lock::holder_identity` + `agent::is_same_process`) is the standard control this phase extends (`terminate_and_verify`), never re-derived from `/proc`-inferred identity (999.47's exact lesson) |
| V5 Input Validation | yes (25c) | Git tag strings and commit messages are attacker-influenced only in a very narrow sense (an untrusted contributor's PR commits could contain crafted conventional-commit-like text) — `semver::Version::parse`/`git-conventional::Commit::parse` both return typed `Result`, and D-10's floors (unrecognised → patch) ensure malformed input degrades safely rather than panicking or mis-parsing into an unintended major bump |
| V6 Cryptography | no | Not touched by this phase (25f is documentation about the EXISTING signing procedure, not a code change to it) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| Signalling the wrong process via a recycled PID (25d's `terminate_and_verify`, discovery scan) | Tampering / Elevation of Privilege (signalling an unrelated user's process on a shared machine) | Never signal on cmdline/argv inference alone (the exact 999.47 lesson) — the registry-independent discovery mechanism recommended in this document matches on a project-specific, literal script fragment (Layer 1) or `argv[0]`+`argv[1]` pair (Layer 2), narrower than 999.47's disproven basename-prefix match; any newly-discovered candidate that a reaper acts on should still be re-verified alive immediately before signalling (TOCTOU — 999.47's own "Related TOCTOU" note) |
| A crafted commit message forcing an unintended major-version bump (25c) | Tampering | D-09's human gate (never-silent, `run_gate`) is the actual mitigation — no unattended run can ship a major bump regardless of how the classifier was fooled; this is defense-in-depth already required by the locked decisions, not a new control this research is proposing |
| A malformed/adversarial tag name crashing the version-derivation path (25c) | Denial of Service (a bad tag blocking every future ship) | `semver::Version::parse` returns `Result`, never panics on malformed input — the reachable-tag filter (`filter_map(...ok())`) silently skips any tag that doesn't parse, which is the correct behavior (a stray non-semver tag like `archive-planning-docs-2026-07-24` must not crash version computation, only be excluded from consideration) |

## Sources

### Primary (HIGH confidence — verified live against this repository's own source and git history in this session)

- `crates/devflow-core/src/version.rs` (full read) — current `compute_version`/`count_git_tags`/`commits_since_last_minor_tag`/`read_version`/`write_version`/`detect_version_file`, and their full test suite
- `crates/devflow-core/src/agent.rs` (full read) — `agent_running`, `terminate`, `process_start_time`, `is_same_process`, `looks_like_devflow_process` and all their tests, including the documented fork/exec-window mechanism
- `crates/devflow-core/src/lock.rs` (full read) — `(pid, starttime)` identity recording and `holder_identity`
- `crates/devflow-core/src/monitor.rs` (spawn_monitor + script construction) — the wrapper shell script's exact structure
- `crates/devflow-core/src/git.rs` (`origin_main_ancestor_status`/`AncestorStatus`, `divergence_from_develop`, `feature_start`, `merge_feature_into_develop`, `is_merged_into_develop`)
- `crates/devflow-core/src/hooks.rs` (`hooks_for_transition`, `hooks_after_ship`, `version_bump`, `merge_feature`)
- `crates/devflow-cli/src/preflight.rs` (full read) — `run_preflight`, `generic_preflight_checks`, `preflight_gh_auth_check`, `phase_reachability_on_base`/`AncestorStatus`-adjacent probe pattern
- `crates/devflow-cli/src/pipeline_launch.rs` (full read) — `launch_stage`/`launch_stage_inner`, exact `enforce_build_staleness` call site
- `crates/devflow-cli/src/commands.rs` (targeted reads: `start` lines 150-248, `gate_sweep`/`stop`/`stop_via_gate`/`stop_via_lock` lines 990-1256, the 25e-relevant tests lines 3259-3458)
- `crates/devflow-cli/src/pipeline_gate.rs` (lines 790-850 — the previously-undocumented `compute_version`/`count_git_tags` consumer)
- `crates/devflow-cli/src/test_support.rs` (full relevant section) — `agent_free_git_only_path_dir`, `stub_agent_binary`, `prepend_path`
- `crates/devflow-core/src/test_support.rs` — `git_command`/`hermetic_command`, the 999.37 per-Command scrubbing precedent
- `CONTRIBUTING.md` (release procedure section, `.gitconfig` section) and tracked `.gitconfig` — both 25f drift claims verified against live file content
- `Cargo.toml`, `crates/devflow-core/Cargo.toml`, `crates/devflow-cli/Cargo.toml`, `Cargo.lock` — direct/transitive dependency inventory
- Live git commands run against this repository in this session: `git tag --list`, `git tag --merged HEAD`, `git merge-base --is-ancestor <tag> HEAD` (all 12 tags), `git cat-file -t <tag>` (annotated-vs-lightweight check), `git log --no-merges` sampling
- `.planning/phases/25-end-to-end-dogfood-blockers/25-CONTEXT.md` (full read) and `.planning/ROADMAP.md` (Phase 25 entry + backlog entries 999.38, 999.44, 999.47, 999.48, 999.49, 999.51)
- `.planning/PROJECT.md`, `.planning/STATE.md` (partial — active-phase/current-position sections)
- `gsd-tools query package-legitimacy check --ecosystem crates semver git-conventional` — live crates.io-backed verdicts for both new dependency candidates
- `cargo info semver` / `cargo info git-conventional` — live crates.io metadata (version, publish date, maintainer, repo)

### Secondary (MEDIUM confidence)

- `$HOME/.claude/skills/gsd-ship/SKILL.md` (read, but only its 24-line delegation stub — the substantive workflow it points to, `$HOME/.claude/gsd-core/workflows/ship.md`, was NOT read in this session; see Open Question 1 / Assumption A1)

### Tertiary (LOW confidence — none used without independent verification in this document; every training-data-only claim below is explicitly tagged `[ASSUMED]` in-line where it appears, not presented as fact)

- None beyond what is already flagged inline as `[ASSUMED]` in the Common Pitfalls / Assumptions Log sections above.

## Metadata

**Confidence breakdown:**
- Standard stack (25c dependencies): HIGH — both `semver` and `git-conventional` verified live via the package-legitimacy seam against the crates.io registry, not from training-data recall
- Architecture (25b/25c/25d integration points): HIGH for the mechanical parts (call sites, signatures, visibility, existing precedents all read directly from live source); MEDIUM for 25c's preflight-timing completeness (Pitfall 1, genuinely open pending Open Question 1) and 25d's root cause (Open Question 2, explicitly not required to be resolved per D-17/999.47's own stated lesson)
- Pitfalls: HIGH — all four documented pitfalls were discovered by reading live source in this session (the `pipeline_gate.rs` hidden consumer, the `ensure_agent_binary` non-Command-based PATH read, the preflight-timing gap, the already-present `libc` dependency), not inferred from the ROADMAP/CONTEXT text alone

**Research date:** 2026-07-27
**Valid until:** ~14 days (this is fast-moving, actively-dogfooded internal infrastructure — the repository's own git history/tag set that 25c's numbers are computed from will have moved by the time this phase executes; re-verify `git tag --merged HEAD`'s output and the 118/120 commit-conformance measurement at plan/execution time rather than trusting this document's snapshot numbers as still current)
