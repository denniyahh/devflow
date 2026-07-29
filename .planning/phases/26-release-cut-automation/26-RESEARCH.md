# Phase 26: Release-Cut Automation - Research

**Researched:** 2026-07-29
**Domain:** Rust CLI (clap) driving `git`/`gh`/`cargo` as subprocesses; no new external dependency required
**Confidence:** HIGH (nearly every claim below is [VERIFIED] by reading this repository's own source and running commands against this exact checkout — this is an internal-automation phase, not a new-library-integration phase)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** `develop`-bound merges (version-bump commit, and the sync-back
  merge) are **direct pushes to `origin/develop`, not PRs.** No human click,
  no `gh pr merge`. The operator (not this phase) will set up a GitHub
  ruleset bypass entry for whatever credential DevFlow pushes with — Phase 26
  assumes that bypass exists and just implements the push; it does not touch
  GitHub settings and does not need to document the ruleset change (the
  operator handles this out-of-band, on their own timeline). Reversible.
- **D-02:** The `develop → main` release PR **stays PR-gated and
  human-merged** for now. `main`'s squash-only merge setting is unchanged by
  this phase.
- **D-03:** `devflow release --yes-release` covers the **entire**
  bump→tag→sync→publish sequence as one typed authorization, mirroring
  `--yes-ship`'s existing all-or-nothing shape and its non-negotiable rule:
  a dangerous operation must be typed per-invocation, never a standing
  default/config flag. This is a **new, separate flag** from `--yes-ship`.
  Reversible.
- **D-04:** `cargo publish` for both crates **is** part of the automated
  sequence (devflow-core, then devflow, per the order `publish_order`
  already computes). The executor queries crates.io before attempting each
  publish, to distinguish "already published at this version, skip" from a
  genuine failure. One-way — a crates.io publish can never be un-published
  or reused at that version.
- **D-05:** The executor follows the **same fail-fast, no-automatic-rollback
  philosophy** already documented for `hooks_after_ship` (Merge→VersionBump→
  ChangelogAppend→BranchCleanup). Whatever succeeded stays; there is no
  automatic undo of a landed commit, tag, or publish. The fix is always
  forward (retry the failed step), never an automatic compensating action.
- **D-06:** Re-running `devflow release --yes-release` after a partial
  failure **auto-skips steps already completed**, detected from live
  git/registry state (tag already exists and is reachable → skip tagging;
  crate already live on the registry at this version → skip that publish;
  `develop` already ahead of the computed bump → skip the bump push) rather
  than requiring the operator to manually diagnose where to resume.
- **D-07:** `devflow sync` (porting `scripts/sync-main-to-develop.sh`) is
  **both a standalone subcommand** and **internally reused** by the executor
  as its own sync step — one implementation, two entry points.
- **D-08:** Sync **direct-pushes to `develop`**, consistent with D-01.
- **D-09:** The script's existing safety check is **preserved exactly**:
  the `-X ours` merge must produce a byte-identical tree to `develop`'s
  pre-merge tree; any mismatch **refuses and leaves `develop` untouched**.
- **D-10 (999.54/999.50 DROPPED from phase AND backlog):** `release --check`'s
  signing-viability *predictor* (`check_ssh_signing_viability`) is **not**
  fixed, extended, or reused by the executor. The operator does not want
  signing-viability *prediction* built into DevFlow, ever — a predictor is a
  second implementation of "will signing work?" that must stay in sync with
  what git actually does. The executor's tag step just **runs the real
  signed `git tag` command** (CONTRIBUTING.md's documented explicit
  key-selection form) and reports git's own real exit code / `git tag -v`
  verification. "Claude can't sign `main`" falls out of D-02 (Claude's
  unattended path stops at `develop`), not from any signing-check code.
- **D-11 (999.4 DROPPED from phase AND backlog):** the concurrent-ship
  tag-race scenario is specific to `devflow parallel` (multiple whole phases
  concurrently), which the operator does not and would never use for a
  single-user setup. Not this phase's concern.
- **D-12:** The changelog entry generated during a release is **derived
  from the same conventional-commit classification** Phase 25's version-bump
  step already computes (feat/fix/docs/etc. over `baseline..HEAD`), replacing
  the hardcoded `"Released phase via DevFlow."` (`ship.rs:394`). See
  `## Common Pitfalls` below — D-12's phrase "reusing data already computed"
  needs a precise reading: the *range-resolution and per-commit-parsing
  machinery* is reused; the *aggregate `Bump` value* `classify_range_bump`
  returns is not itself changelog content and cannot be used directly.

### Claude's Discretion

- Exact shape of the `--execute`/`--yes-release` CLI surface (new `Release {
  execute, yes_release, ... }` args vs. a distinct `Command::ReleaseExecute`
  variant) — match the existing `Release { check, project }` pattern.
- Where the direct-push code lives (new `GitFlow` method alongside
  `merge_feature_into_develop`, vs. a standalone function) — follow
  established module conventions in `git.rs`.
- How `devflow sync`'s standalone-vs-internal duality is implemented (shared
  function called from both a CLI command handler and the executor's
  internal sequence).
- Retry/backoff shape for the crates.io pre-publish check (D-04) — a single
  synchronous query is sufficient; no polling loop implied.
- Doc-comment and CONTRIBUTING.md updates reflecting which of the 7 manual
  "Cutting a Release" steps `--yes-release` now covers.

### Deferred Ideas (OUT OF SCOPE)

- `devflow parallel`'s future (remove whole-phase concurrency, repurpose for
  intra-phase parallelism, or leave alone) — needs its own phase.
- `gh pr merge`-driven auto-merge for the `develop → main` release PR —
  revisit once Claude-based PR review exists as a real capability.
- 999.55 (`phase7_cli::wait_for` fixed timeout) and 999.39 (production git
  calls don't scrub `GIT_DIR`) — explicitly excluded, handled elsewhere.
</user_constraints>

<phase_requirements>
## Phase Requirements

This project tracks this phase by backlog identifier, not `REQ-`-prefixed
IDs (confirmed: `.planning/REQUIREMENTS.md` does not exist in this project —
DevFlow uses ROADMAP.md phase entries + CONTEXT.md decisions instead).

| ID | Description | Research Support |
|----|-------------|------------------|
| 999.52 | `devflow sync` subcommand (standalone + executor-internal), porting `scripts/sync-main-to-develop.sh`'s `-X ours` + tree-identity logic | `## Architecture Patterns` Pattern 2; `## Code Examples` "Porting the sync script" |
| 999.5 | Replace hardcoded changelog string with conventional-commit-derived content | `## Common Pitfalls` Pitfall 1; `## Code Examples` "Grouped changelog content" |
| 999.25 | `devflow release --execute`/`--yes-release`: version bump → push develop → (human PR to main) → signed tag → sync → publish | `## Architecture Patterns` Pattern 1, 3, 4; `## Common Pitfalls` all; `## Don't Hand-Roll` |

Sequencing per CONTEXT.md: 999.52 first (999.25's sync step calls it),
then 999.5 (small, independent), then 999.25 (composes both, plus new
develop-push/tag/publish code).
</phase_requirements>

## Summary

This phase adds real push/tag/publish automation to a codebase that
currently has **none** — not a smaller version of an existing mechanism, but
the first production code that ever calls `git push`, `git tag -s`, or
`cargo publish`. Concretely verified: `GitFlow::push()` and
`GitFlow::delete_remote_branch()` (`git.rs:209,222`) exist but have **zero
non-test call sites** anywhere in `crates/`; no `Hook` or CLI command ever
invokes them. `GitFlow::release_start`/`release_finish` (`git.rs:114-138`,
an older git-flow release-branch dance that tags with `-c tag.gpgSign=false`
and merges into both `main` and `develop` directly) are similarly dead in
production — tests only. `CHANGELOG.md`'s own 2.1.0 entry states this
plainly: *"the release cut itself (opening the develop → main PR, tagging,
publishing) is deliberately manual."* This phase is the one that stops that
being true, in the specific, narrower shape D-01–D-12 define (develop gets
direct pushes; `main` keeps its human-merged PR gate).

A separate, already-wired mechanism — `hooks_after_ship()` (Merge→
VersionBump→ChangelogAppend→BranchCleanup, `hooks.rs:105`) — already computes
a version via `compute_version`, writes it, and creates a **local, unsigned,
lightweight** tag (`GitFlow::tag`, `-c tag.gpgSign=false`) at the end of
every phase's Ship stage, entirely inside `ctx.project_root` (the main
checkout, never the phase's worktree) and never pushed anywhere. This is a
distinct code path from what this phase builds, but it shares the exact tag
*naming* scheme (`v{compute_version()}`) the new release executor's tag
step will also use. See Pitfall 1 below — this is a real, non-hypothetical
collision surface that the plan must design around, not an incidental detail.

The crates.io "already published?" check (D-04/D-06) has a clean answer that
adds **no new dependency**: `cargo info <name>@<version>` — stable since
Rust 1.82, present in this workspace's pinned toolchain (1.97.1) — exits `0`
if that exact version is live on the registry and `101` with a
`could not find` stderr message if not (both behaviors verified live against
crates.io from this exact checkout). No HTTP client crate (`reqwest`,
`ureq`, etc.) needs to be added; `devflow-core`'s `Cargo.toml` has none
today and none of the existing dependencies (`libc`, `serde`, `serde_json`,
`toml`, `thiserror`, `tracing`, `semver`, `git-conventional`) can make a
network call — shelling out to `cargo`, already a hard build/runtime
requirement, is the only option consistent with the "don't add a heavy new
dependency" guidance and the project's existing "shell out to the real
tool" pattern (`gh auth status` in `preflight.rs`, `git` everywhere in
`git.rs`).

**Primary recommendation:** treat this phase as "wire up push/tag/publish
for the first time, under the existing fail-fast/no-rollback/idempotent-
resume philosophy already proven for `hooks_after_ship`" — reuse that
policy's shape, not its code (the Ship-time hooks operate on local-only
state; this phase's operations are inherently remote-mutating and must be
designed for that from the start, including an idempotency check per step
that reads live git/registry state, exactly as D-06 specifies).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| CLI flag surface (`--execute`/`--yes-release`) | CLI (`devflow-cli`) | — | Matches existing `Release { check, project }` pattern; clap owns argument parsing |
| Direct push to `develop` (version bump, sync) | Core (`devflow-core::git`) | CLI (orchestration) | `GitFlow` already owns every git mutation; CLI only sequences calls and reports |
| `devflow sync` (standalone + internal) | Core (shared fn) | CLI (two thin entry points) | D-07 requires one implementation, two callers — core owns the logic, CLI wires the subcommand and the executor call site |
| Signed tag creation | Core (`devflow-core::git`, new fn) | — | Same tier as existing `GitFlow::tag`; must NOT touch `check_ssh_signing_viability` (D-10) |
| Changelog content generation | Core (`devflow-core::version` or `ship`) | — | Reuses `version.rs`'s git-log-parsing idiom; output consumed by `ship::prepend_changelog` |
| crates.io "already published?" check | Core (new fn, shells to `cargo info`) | — | Same "shell to the real tool" pattern as everything else in `git.rs` |
| `cargo publish` invocation, in `publish_order` | Core (new fn) or CLI | — | Either tier is defensible; keep alongside `publish_order` in `git.rs` for cohesion (Claude's discretion, per CONTEXT.md) |
| Release-cut sequencing/idempotent resume (D-06) | CLI (`commands.rs`, new `release_execute`) | Core (per-step state-read helpers) | Mirrors `release_check`'s existing shape: CLI orchestrates a list of `Check`-like steps, core supplies the read-only predicates each step consults before acting |
| `main`-branch PR merge | Human (out of tier) | — | D-02: explicitly NOT automated this phase |

## Standard Stack

### Core

No new external dependency is introduced by this phase. Every capability is
built from tools already required to build/run DevFlow:

| Tool | Version (verified, this env) | Purpose | Why Standard |
|------|-------------------------------|---------|---------------|
| `git` (subprocess) | 2.55.0 | push, tag, merge, log — all via `Command::new("git")`, matching every existing `git.rs` function | Established project-wide pattern; no `git2`/`libgit2` binding is used anywhere in this codebase |
| `cargo` (subprocess) | 1.97.1 (pinned, `rust-toolchain.toml`) | `cargo info <pkg>@<ver>` for D-04's pre-publish existence check; `cargo publish -p <pkg>` for the actual publish | `cargo info` stabilized in Rust 1.82 [CITED: doc.rust-lang.org/cargo/commands/cargo-info.html, cross-referenced against InfoWorld's Rust 1.82 coverage] — well within the pinned 1.97.1 toolchain |
| `gh` (subprocess, optional) | 2.96.0 | Not newly invoked by this phase (D-02: no `gh pr create`/`gh pr merge` added) — listed because `devflow doctor`/preflight already probe it and the release flow's human PR step depends on it existing in the operator's environment | Existing precedent: `preflight.rs:617`, `commands.rs` doctor check |
| `git_conventional` (existing crate) | `"1"` (already in `devflow-core/Cargo.toml`) | Per-commit type classification for changelog grouping (D-12) | Already used by `classify_commit_message` (`version.rs:429`) — the changelog generator reuses this same parser, just retains messages instead of collapsing to a max |
| `semver` (existing crate) | `"1"` | Tag/version comparisons (tag-exists/tag-reachable checks reuse `highest_semver_tag`/`reachable_semver_baseline`) | Already the project's only semver dependency |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `thiserror` (existing) | `"2"` | New error variants (e.g. a `SyncError`/extension to `GitError`) for the sync subcommand and publish-check failures | Matches every existing error enum in this codebase |
| `tracing` (existing) | `"0.1"` | `info!`/`warn!` structured events for each release-cut step (mirrors `hooks.rs`'s `Merge`/`VersionBump` logging) | Consistency with CONTRIBUTING.md's logging conventions section |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `cargo info <pkg>@<ver>` (shell out) | A crates.io HTTP client crate (`crates_io_api`, or a raw sparse-index GET via `reqwest`/`ureq`) | Adds a new dependency (and for `reqwest`, a TLS stack) purely to answer a question `cargo` — already required — answers for free. `cargo info`'s output is line-oriented and its exit code is coarse (101 = generic error, disambiguated only by matching stderr text), which is a real but acceptable fragility (see Pitfall 4) against the alternative of a new HTTP dependency in a codebase that has deliberately avoided one everywhere else (`version.rs`'s doc comment: "this project deliberately avoids a TOML parser dependency… for its version/workspace tooling") |
| Direct `git push origin develop` | `git push --force-with-lease` | Force-pushing `develop` is never correct here — D-01/D-08 are both fast-forward-style pushes (a version-bump commit added on top of current `develop`, or a `-X ours` merge that is a strict fast-forward by construction since it starts from the current tip). No force flag should ever appear in this phase's push calls; if a push is rejected (non-fast-forward), that is a genuine "someone moved `develop`" case D-05's fail-fast policy should surface as an error, not paper over |
| A new signing-viability predictor reused/fixed from 999.54/999.50 | Running the real `git tag -s ... && git tag -v ...` and reading its actual result | D-10 is explicit and non-negotiable: no predictor, ever. This is the one "alternative" the planner must NOT reach for even though it is the closest-looking precedent in the codebase (`check_ssh_signing_viability`, `git.rs:811`) |

**Installation:** none required — every tool above is either an existing
workspace dependency or a subprocess (`git`, `cargo`, `gh`) already a hard
requirement to build/run/release this project.

**Version verification performed:**
```
$ cargo --version
cargo 1.97.1 (c980f4866 2026-06-30)
$ git --version
git version 2.55.0
$ gh --version
gh version 2.96.0 (2026-07-02)
```
`cargo info` behavior verified live against the real crates.io registry from
this checkout (see Code Examples).

## Package Legitimacy Audit

**No new external packages are introduced by this phase.** Every dependency
used (`git`, `cargo`, `gh` as subprocesses; `git_conventional`, `semver`,
`thiserror`, `tracing` as existing workspace crates) is already present in
`Cargo.toml`/`Cargo.lock` and was vetted in prior phases. The Package
Legitimacy Gate protocol (`gsd-tools query package-legitimacy check`) does
not apply — there is nothing to check.

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|--------------|---------|-------------|
| *(none — no new packages)* | — | — | — | — | — | N/A |

**Packages removed due to `[SLOP]` verdict:** none.
**Packages flagged as suspicious `[SUS]`:** none.

## Architecture Patterns

### System Architecture Diagram

```
                    devflow release --execute --yes-release
                                   │
                                   ▼
                 ┌─────────────────────────────────────┐
                 │  release_execute() (commands.rs)     │
                 │  reuses release_check()'s 4 checks   │
                 │  as a hard pre-gate (fail fast)       │
                 └───────────────┬───────────────────────┘
                                 ▼
        ┌────────────────────────────────────────────────────┐
        │ STEP 1: Version bump                                │
        │  compute_version() ─┐                                │
        │  write_version()    │  (existing, Phase 25)          │
        │  commit_path()      │                                │
        │  NEW: git push origin develop  ◄── D-01, first ever  │
        │  Idempotency (D-06): is develop already at/ahead of   │
        │  the computed version? → skip push                   │
        └───────────────┬────────────────────────────────────┘
                         ▼
        ┌────────────────────────────────────────────────────┐
        │ STEP 2: Human gate (OUT OF AUTOMATION — D-02)        │
        │  Operator opens develop→main PR, CI green,           │
        │  squash-merges (existing GitHub ruleset, unchanged)  │
        └───────────────┬────────────────────────────────────┘
                         ▼
        ┌────────────────────────────────────────────────────┐
        │ STEP 3: Signed tag (NEW — first tag-creation ever    │
        │  in production code)                                 │
        │  git -c user.signingkey="$(git config --get           │
        │    devflow.releaseSigningKey)" tag -s vX.Y.Z <sha>   │
        │    -m "vX.Y.Z"                                        │
        │  git push origin vX.Y.Z                               │
        │  Verify: git tag -v vX.Y.Z (real result, no predictor)│
        │  Idempotency (D-06): tag exists AND is annotated AND  │
        │  reachable from main AND already on origin → skip    │
        └───────────────┬────────────────────────────────────┘
                         ▼
        ┌────────────────────────────────────────────────────┐
        │ STEP 4: Sync (devflow sync — 999.52, D-07/D-08/D-09) │
        │  fetch origin main develop                            │
        │  origin/main ancestor of develop? → no-op, done       │
        │  else: git merge -X ours origin/main --no-edit         │
        │  tree-identity check: BEFORE == AFTER? else ABORT      │
        │  git push origin develop  ◄── D-08, direct push        │
        └───────────────┬────────────────────────────────────┘
                         ▼
        ┌────────────────────────────────────────────────────┐
        │ STEP 5: Publish (NEW — cargo publish never run by    │
        │  any DevFlow code before this phase)                 │
        │  for pkg in publish_order(project_root):              │
        │    cargo info pkg@version → exit 0? skip (D-06)       │
        │    exit 101 + "could not find"? → cargo publish -p pkg│
        │    other failure → fail-fast, no rollback (D-05)      │
        └────────────────────────────────────────────────────┘

Changelog content (999.5) is generated as part of STEP 1 (before the
version-bump commit), consumed by ship::prepend_changelog, replacing the
"Released phase via DevFlow." literal.
```

### Recommended Project Structure

No new modules are required; extend existing files along their established
seams:

```
crates/devflow-core/src/
├── git.rs           # + push_develop()/similar, + tag_signed(), + is_tag_...
│                    #   reachable/annotated helpers, + cargo_info_check(),
│                    #   + cargo_publish() — alongside publish_order()
├── sync.rs          # NEW — devflow sync's shared logic (D-07's one impl),
│                    #   ported from scripts/sync-main-to-develop.sh
├── version.rs       # + changelog-grouping function (new, alongside but
│                    #   NOT replacing classify_range_bump — see Pitfall 1)
├── ship.rs          # prepend_changelog(...) signature likely grows to
│                    #   accept the new structured content, replacing the
│                    #   hardcoded "Released phase via DevFlow." bullet
└── hooks.rs         # unchanged — this is Ship-stage machinery, a
                     #   different lifecycle event from Release

crates/devflow-cli/src/
├── main.rs          # Command::Release grows execute/yes_release fields;
│                    #   new Command::Sync variant (or folds into Release —
│                    #   Claude's discretion per D-07)
└── commands.rs       # release_execute() alongside existing release_check();
                     #   sync_cmd() alongside it
```

### Pattern 1: Idempotent step via live-state check, not a progress file (D-06)

**What:** Every release-cut step is guarded by a read-only predicate that
inspects live git/registry state before acting, mirroring the sync script's
own `git merge-base --is-ancestor origin/main HEAD` short-circuit.
**When to use:** Every one of the five steps in the sequence diagram above.
**Example (existing precedent this pattern extends):**
```rust
// Source: scripts/sync-main-to-develop.sh:41-44 (the actual, already-
// deployed precedent for this idempotency style)
if git merge-base --is-ancestor origin/main HEAD; then
    echo "origin/main is already an ancestor of develop — nothing to sync."
    exit 0
fi
```
The release executor's five steps each need an equivalent Rust predicate:
"is `develop`'s tip already at/past the computed version?", "does tag
`vX.Y.Z` already exist AND is it annotated AND does `git tag -v` pass AND is
it on `origin`?", "does `cargo info pkg@version` exit 0?". None of these
should be backed by a persisted `.devflow/release-state.json` — D-06 is
explicit that resume semantics come from live state, not a checkpoint file.

### Pattern 2: Porting a proven shell script into Rust, preserving every check (999.52)

**What:** `scripts/sync-main-to-develop.sh` is fully proven in this exact
repository (it fixed the v1.5.0 divergence and is the documented fix for
v2.0.0's). Port its checks 1:1, don't redesign them.
**Checks that MUST survive the port** (read the script in full — already
done for this research, reproduced here for the plan to verify against):
1. Working tree clean (`git status --porcelain` empty) — refuse otherwise.
2. Currently on `develop` — refuse otherwise (`git rev-parse --abbrev-ref HEAD`).
3. `git fetch origin main develop --quiet` runs before any comparison.
4. Short-circuit: `git merge-base --is-ancestor origin/main HEAD` — if true,
   print "nothing to sync" and stop (exit 0 for the standalone command; for
   the executor's internal call this becomes "step already satisfied, move
   on" per D-06).
5. `BEFORE_TREE=$(git rev-parse HEAD^{tree})` captured before the merge.
6. `git merge -X ours origin/main --no-edit -m "<message>"`.
7. `AFTER_TREE=$(git rev-parse HEAD^{tree})` captured after.
8. **Tree-identity check is the load-bearing safety property (D-09):** if
   `BEFORE_TREE != AFTER_TREE`, refuse — leave `develop` untouched, do not
   push, print which SHAs changed and why. This is what makes `-X ours`
   trustworthy: it's not "assume ours always wins cleanly," it's "verify
   ours won completely, or abort."
9. **D-08's addition (not in the shell script):** on success, `git push
   origin develop` — the shell script's own final instruction was "Push
   with: git push origin develop", left as a manual step because `develop`
   was protected; this phase automates exactly that last line.

### Pattern 3: The signed tag step is verify-by-doing, never verify-by-predicting (D-10)

**What:** Run the exact command CONTRIBUTING.md already documents, verbatim
— do not build a second "will this work" check.
**Example:**
```bash
# Source: CONTRIBUTING.md § "Cutting a Release", step 5 (verbatim, this is
# the form D-10 requires — never a bare `git tag -s`)
git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" \
    tag -s vX.Y.Z <commit> -m "vX.Y.Z"
git push origin vX.Y.Z
```
Read git's own exit code from the `tag -s` invocation. If it's non-zero,
that's the release-cut's real, authoritative failure — surface it verbatim
(redacted per this project's existing information-disclosure discipline,
`T-17-13`/`T-25-52` precedent in `preflight.rs`) and stop (D-05, no
rollback, no retry-with-different-key). Verification after the fact uses
`git tag -v vX.Y.Z`, which itself is git actually checking the signature —
still "doing," not "predicting."

### Pattern 4: `cargo info` as the crates.io existence oracle (D-04)

**What:** Shell to `cargo info <pkg>@<version>`; classify by exit code +
stderr substring.
**Example (live-verified against the real registry from this checkout):**
```
$ cargo info devflow-core@1.8.0 --registry crates-io   # exit 0
    Updating crates.io index
devflow-core #cli #workflow #automation #git #devops
...
version: 1.8.0 (latest 2.1.0)
...

$ cargo info devflow-core@0.0.1                        # exit 101
    Updating crates.io index
error: could not find `devflow-core@0.0.1` in registry \
  `https://github.com/rust-lang/crates.io-index`
```
```rust
// Illustrative shape — not verbatim project code, follows git.rs's
// existing Command::new(...).output() idiom throughout this file.
fn crate_already_published(name: &str, version: &str) -> Result<bool, PublishCheckError> {
    let output = Command::new("cargo")
        .args(["info", &format!("{name}@{version}"), "--registry", "crates-io"])
        .output()?;
    if output.status.success() {
        return Ok(true); // exact version already live — D-06: skip publish
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("could not find") {
        return Ok(false); // genuinely not yet published — proceed
    }
    // Anything else (network failure, registry outage, auth issue) is a
    // real error, not "not published yet" — D-05 fail-fast, do not guess.
    Err(PublishCheckError::Ambiguous(stderr.trim().to_string()))
}
```

### Anti-Patterns to Avoid

- **Reusing `GitFlow::release_start`/`release_finish` as a shortcut.** These
  exist in `git.rs:114-138` and look like an obvious fit ("release" is
  right there in the name) but they (a) tag unsigned/lightweight
  (`-c tag.gpgSign=false`), (b) merge into both `main` and `develop`
  directly with no PR gate, and (c) have zero production callers today —
  they are an older git-flow release-branch pattern this project no longer
  actually uses. Building on them would silently reintroduce unsigned tags
  and bypass D-02's human-gated `main` merge.
- **Reusing `hooks_after_ship`'s `version_bump`/`tag` as the release
  executor's version-bump/tag step.** These operate on local-only state at
  Ship time (a different lifecycle event, merging a *phase* branch into
  local `develop`) and were never designed to push anything. The release
  executor needs its own step that computes the version (reusing
  `compute_version` — that part is correct to share) but then *pushes*,
  which `hooks_after_ship` never does and isn't structured to do.
- **Treating `classify_range_bump`'s returned `Bump` as changelog content.**
  It's a single aggregate enum value (`None`/`Patch`/`Minor`/`Major`), not a
  list of what changed. See Pitfall 1.
- **Force-pushing anywhere.** No step in this sequence should ever need
  `--force`/`--force-with-lease`. A rejected push (non-fast-forward) is a
  genuine error to surface, not a conflict to overwrite.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| crates.io "is this version live?" | A registry HTTP client / sparse-index parser | `cargo info <pkg>@<ver>` (subprocess) | Cargo is a required build tool; it already knows how to talk to the registry (auth, index format, mirrors) correctly. Rolling an HTTP client adds a dependency this project has deliberately avoided everywhere else |
| Tag-signing-will-it-work prediction | A second `check_ssh_signing_viability`-style predictor scoped to the release path | Just run `git tag -s ...` and read its real exit code | D-10, non-negotiable. A predictor is *by definition* a second implementation that can drift from git's real behavior — the exact bug class 999.50/999.54 existed to fix, permanently avoided by never building it |
| `-X ours` merge safety | A custom diff/conflict-resolution heuristic | Byte-identical BEFORE/AFTER tree comparison (`git rev-parse HEAD^{tree}`) | Already proven correct in `scripts/sync-main-to-develop.sh`; a hand-rolled "smart" merge-conflict resolver would be strictly worse and unproven |
| Publish ordering | A second dependency-graph walk | `devflow_core::git::publish_order` (`git.rs:516`) | Already correct, already tested (`publish_order_derives_core_before_cli_from_a_fixture_workspace`), handles both inline and long-form `[dependencies.X]` manifest syntax |
| Conventional-commit parsing | A regex-based commit-message parser | `git_conventional::Commit::parse` (already a dependency, already used by `classify_commit_message`) | Already vetted and in the dependency tree; a second parser risks disagreeing with the version-bump classifier on the same commits |

**Key insight:** every "don't hand-roll" item above has a name-brand-looking
in-repo alternative that is *wrong for this specific reason* (unsigned tags,
a predictor, an aggregate instead of a list) rather than merely
suboptimal — the risk in this phase is reaching for the nearest-looking
existing code rather than the actually-correct existing code.

## Common Pitfalls

### Pitfall 1: `classify_range_bump` is not changelog content — it's a single aggregate value

**What goes wrong:** A plan that says "call `classify_range_bump` and use
the result for the changelog" will compile and produce... a single
`Bump::Minor`/`Bump::Patch`/etc. value, which is not a list of what
changed. `classify_range_bump` (`version.rs:393-423`) deliberately reduces
every commit in the range to `bump.max(this_bump)` — it never retains
individual commit messages, types, or subjects past that fold.
**Why it happens:** D-12's wording ("derived from the same conventional-
commit classification… already computes") is easy to over-read as "call the
existing function and get content back."
**How to avoid:** The plan needs a **new** function (e.g.
`group_commits_by_type(project_root, range_start) -> BTreeMap<Type,
Vec<String>>` or similar) that walks the *same* range with the *same*
`git log --no-merges <range> --format=%H%x1f%B%x1e` idiom and the *same*
`git_conventional::Commit::parse` call, but collects subjects into groups
instead of folding to a max. `release_range_start` (the anchor-resolution
logic, `version.rs:301-357`) and `reachable_semver_baseline`/
`highest_semver_tag` (the baseline lookup) genuinely are reused as-is — it
is specifically the aggregation step that must be new, sibling code, not a
literal reuse.
**Warning signs:** A plan task whose acceptance criterion is "changelog
entry is non-empty" without specifying which conventional-commit types map
to which changelog heading (Added/Fixed/Changed, or similar) — that
ambiguity is exactly where a plan silently regresses to "just dump the
Bump enum's Debug output" or similarly useless content.

### Pitfall 2: A local, unsigned, lightweight `v{version}` tag may already exist with the release tag's exact name

**What goes wrong:** `hooks_after_ship`'s `version_bump` hook
(`hooks.rs:268-296`) already runs `GitFlow::tag(&format!("v{version}"))` —
an unsigned, lightweight tag (`-c tag.gpgSign=false`, no `-a`/`-s`/`-m`) —
at the end of *every* phase's Ship stage, in `ctx.project_root` (the shared
main checkout, not the disposable phase worktree). If the release
executor's own tag-creation step (`git tag -s vX.Y.Z ...`) runs later in the
*same* checkout and a tag of that exact name already exists (even a stray
lightweight one), git refuses with `fatal: tag 'vX.Y.Z' already exists`
unless `-f` is passed — and this repository has direct, if slightly
ambiguous, evidence this class of thing has happened before: of this
repo's 12 `vMAJOR.MINOR.PATCH` tags, 11 are annotated (`git cat-file -t`
reports `tag`, i.e. real signed release tags) and **one, `v1.3.69`, is a
lightweight tag** (`git cat-file -t` reports `commit`) that does not
correspond to any released version in `CHANGELOG.md`. [VERIFIED: `git
cat-file -t` run against every local tag in this checkout] The most likely
origin is the 999.37 incident (a test fixture's `GIT_DIR` leaking into the
real repo and creating spurious refs, per `CONTRIBUTING.md`'s pre-push hook
section) rather than `hooks_after_ship` actually firing against this repo —
but the *mechanism* that could produce exactly this collision is real,
present, and unexercised-in-anger, which is a more dangerous state than
"known and already worked around."
**Why it happens:** Two independent code paths (Ship-time bookkeeping tag,
Release-time signed tag) compute the same name from the same
`compute_version()` function and both call `git tag` against the same
shared ref namespace, without either knowing about the other.
**How to avoid:** D-06's "tag already exists → skip tagging" idempotency
check must NOT treat mere existence as sufficient. It should check: (1) is
the existing ref an **annotated** tag (`git cat-file -t <tag>` == `tag`,
not `commit`)? (2) does `git tag -v <tag>` verify successfully (a real
signature, from the expected key)? (3) is it reachable from the commit
being released and already pushed to `origin`? Only if all three hold
should the step be skipped as "already done." A lightweight or unsigned
tag with the target name found locally should be treated as a
non-blocking artifact from a different mechanism — surfaced as a warning,
and either deleted-then-recreated (`git tag -f`, scoped exactly to this
one name after confirming it is NOT the annotated real tag) or the
executor should refuse and ask a human to resolve the name collision,
consistent with D-05's "never automatically compensate" posture.
**Warning signs:** `git tag -s vX.Y.Z ...` failing with "already exists" on
a fresh checkout that has never had a real release cut before; `git tag -v`
reporting "not a valid tag" or "no signature found" for what the executor
believed was an existing release tag.

### Pitfall 3: `cargo info`'s exit code is coarse — 101 covers every error, not just "not found"

**What goes wrong:** Treating any non-zero exit as "not yet published,
proceed to publish" will also fire on a genuine network outage, registry
rate-limit, or offline environment, incorrectly proceeding to attempt a
publish that then fails for an unrelated, confusing reason (or worse, if
retried blindly, wastes a `cargo publish` attempt against a registry that's
simply unreachable).
**Why it happens:** `cargo info` uses `exit(101)` for essentially all
non-success outcomes; the *only* signal that distinguishes "definitely not
published" from "something else went wrong" is matching `could not find`
in stderr — a documented-by-observation, not documented-by-contract,
string.
**How to avoid:** Match on the stderr substring explicitly (as shown in
Pattern 4's example) and treat anything else — including a `cargo info`
spawn failure — as a hard error per D-05 (fail-fast, no guessing). Do not
collapse "ambiguous" and "not found" into the same branch.
**Warning signs:** A publish attempt proceeding when `cargo info` failed
for a reason unrelated to the package's existence (e.g., a transient DNS
failure printed as `error: failed to fetch...`).

### Pitfall 4: `develop`'s existing protection (and CONTRIBUTING.md's own current text) directly contradicts D-01/D-08 until the operator's ruleset bypass exists

**What goes wrong:** `CONTRIBUTING.md`'s "Cutting a Release" section
currently states, in its own words: *"Both `develop` and `main` are
protected branches — direct pushes are rejected... even for the
maintainer."* If the release executor's code is written and tested against
a repository where that ruleset bypass has not yet been configured, every
real invocation of the new develop-push code will fail with GitHub's
"Changes must be made through a pull request" rejection — which will look
identical to a real git push failure the fail-fast policy (D-05) should
correctly report, but the *cause* is an environment precondition, not a
code defect.
**Why it happens:** D-01 explicitly scopes the ruleset bypass as the
operator's own out-of-band responsibility, "not this phase's scope" — so
the code is correctly built to just do a push, but the plan must not
assume the bypass exists at test/verification time.
**How to avoid:** The plan's verification step for the develop-push code
should not require a live push to `origin/develop` on the real repository
to prove the push logic is correct — use a local bare-remote fixture
(`git init --bare`, add as a remote, push there) exactly like the existing
`git.rs` test-fixture pattern (`init_repo()`/`flow()`, `git.rs:969-987`)
does for every other git-mutating function. Document, as part of this
phase's CONTRIBUTING.md update (Claude's discretion item), that a live
`--yes-release` run additionally requires the operator's ruleset bypass to
already exist — this is an environment precondition to surface, not a code
gap to close.
**Warning signs:** A plan task whose verification step is "push succeeds
against the real `origin/develop`" without a fixture-remote fallback for
environments where the bypass isn't configured yet.

### Pitfall 5: `git push` may prompt for credentials/2FA/passphrase in a genuinely unattended run

**What goes wrong:** Whatever credential DevFlow authenticates as (SSH key,
`gh auth`-backed HTTPS token, etc.) must already be non-interactively
usable in the environment `--yes-release` runs in, or the push (and, for
the tag, the signing operation itself) will hang waiting for input that
never arrives — the exact class of failure Phase 13's dogfood finding
already documented for tag-signing (`git.rs:130-132`'s comment about
`$EDITOR` blocking a headless run).
**Why it happens:** `--yes-release`, like `--yes-ship`, is designed for
unattended execution; any interactive credential prompt defeats that by
construction.
**How to avoid:** This is an environment precondition (document in
CONTRIBUTING.md / `devflow doctor`'s existing checks, per the `gh auth
status` precedent) rather than something the code can detect and route
around cleanly — flag as an assumption the operator's release-signing
environment (already documented as pre-configured per D-10's reasoning) is
also configured for non-interactive git push/signing.

## Code Examples

### Porting the sync script's tree-identity check (999.52 core safety property)

```rust
// Source: scripts/sync-main-to-develop.sh:46-63, this is the exact logic
// to preserve — reproduced here in shell for the plan to translate,
// following git.rs's existing Command::new("git").args([...]) idiom.
let before_tree = git_output(["rev-parse", "HEAD^{tree}"])?;
// git merge -X ours origin/main --no-edit -m "<standard sync message>"
git(["merge", "-X", "ours", "origin/main", "--no-edit", "-m", MESSAGE])?;
let after_tree = git_output(["rev-parse", "HEAD^{tree}"])?;
if before_tree != after_tree {
    // Refuse — leave develop untouched, do not push. This is the one
    // check that must never be relaxed (D-09).
    return Err(SyncError::TreeChanged { before_tree, after_tree });
}
git(["push", "origin", "develop"])?; // D-08 — new: the script's own final
                                       // manual instruction, now automated
```

### Existing test-fixture pattern to extend for the new push-mutating tests

```rust
// Source: crates/devflow-core/src/git.rs:969-987 (existing, verbatim
// pattern already used by every GitFlow test in this file)
fn init_repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    commit_file(root, "README.md");
    git(root, &["branch", "-M", "main"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
    dir
}
// For push-testing, add a second `init_repo()`-style bare dir as a
// `git remote add origin <bare-path>` target — this is the standard way
// to test `git push` without a real network dependency, and matches this
// project's stated preference for hermetic git fixtures throughout.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual 7-step CONTRIBUTING.md checklist for every release | `devflow release --check` (read-only preflight only) | Phase 20 (v1.7.0) | Catches 3 of 4 historical failure classes before the cut starts, but the cut itself stays 100% manual — this phase changes that |
| `count_git_tags` + `commits_since_last_minor_tag` (raw tag count / describe-distance versioning) | `highest_semver_tag`/`reachable_semver_baseline` + `classify_range_bump` (conventional-commit-derived) | Phase 25 (D-07/D-08) | Both old functions are `#[deprecated]` but retained for published-crate API compatibility — do not call them in new code |
| `check_ssh_signing_viability` predicting whether signing will succeed | Running the real signed `git tag -s` command and reading its actual result | This phase (D-10) | The predictor function (`git.rs:811`) is retained (999.27's fix is historical, unaffected) but must not gain a new caller from this phase |

**Deprecated/outdated:**
- `version::count_git_tags`, `version::commits_since_last_minor_tag` — superseded by D-07/D-08's git-tag-reachability + conventional-commit scheme; `#[deprecated]` in source, kept only for the published crate's semver API compatibility.
- `GitFlow::release_start`/`release_finish` — an older git-flow release-branch pattern with zero production callers; do not build on these (see Anti-Patterns).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The operator's GitHub ruleset bypass (D-01's precondition) will be configured before any real `--yes-release` run, and the plan does not need to detect its absence programmatically beyond a normal push-failure error path | Pitfall 4 | If the bypass is misconfigured, the push fails with a GitHub-specific rejection message that D-05's generic fail-fast handling will report correctly but unhelpfully (operator has to recognize it's a ruleset issue, not a DevFlow bug) — low risk, since D-01 explicitly assigns this to the operator |
| A2 | The stray `v1.3.69` lightweight tag originates from the 999.37 test-sandbox-escape incident rather than a real historical firing of `hooks_after_ship`'s Ship-time tag creation against this repository | Pitfall 2 | If wrong (i.e. `hooks_after_ship` has actually fired against this repo's real `develop` before), the collision risk described in Pitfall 2 is not merely a design consideration but has already partially manifested — worth a quick operator confirmation before or during planning, since it changes "design defensively" into "this WILL recur on the next phase Ship unless addressed" |
| A3 | No environment variable or `cargo login`-stored credential issue will block a headless `cargo publish` in the operator's real release environment (this was not verified in this sandbox, which has no `CARGO_REGISTRY_TOKEN`/publish credentials configured) | Pitfall 5, Environment Availability | If wrong, the publish step hangs or fails on a credential prompt exactly like the tag-signing `$EDITOR` hazard Phase 13 already found for a different operation |
| A4 | `cargo info`'s `could not find` stderr substring is stable across the pinned toolchain version and will not change wording in a future `rustup` update within this project's support window | Pitfall 3 | If cargo's wording changes, the "not published yet" branch would misclassify as "ambiguous error," which is the fail-safe direction (blocks publish rather than double-publishing), so the failure mode of this assumption being wrong is safe but noisy |

**If this table is empty:** N/A — see rows above; most claims in this
research were verified directly against this repository's source and this
environment's installed tools, which is why the table is short relative to
a typical new-library-integration phase.

## Open Questions

1. **Does `hooks_after_ship`'s Ship-time `VersionBump`/tag creation actually
   run against this repository's real `develop` in current practice, or has
   every historical release been produced entirely through the manual
   CONTRIBUTING.md checklist with `hooks_after_ship` never having reached
   completion in a real dogfood run?**
   - What we know: `STATE.md` (Phase 23 context, 2026-07-25) states "no
     phase has ever completed a full five-stage devflow-driven run";
     `CHANGELOG.md`'s 2.1.0 entry states the release cut is "deliberately
     manual" as of today. Both point toward "never actually fired against
     the real repo," which would make Pitfall 2's collision risk purely
     theoretical rather than actively recurring.
   - What's unclear: whether a *partial* dogfood run (one that reached Ship
     but not further) has fired `hooks_after_ship` against the real
     `project_root` checkout even without a full end-to-end run completing.
   - Recommendation: the planner should ask the operator directly (cheap,
     one question) rather than have the executor guess — if confirmed to
     have fired before, Pitfall 2's mitigation becomes mandatory-verify-first
     rather than defensive-design; if confirmed never to have fired,
     Pitfall 2's guidance still stands but is lower urgency for the very
     first release cut this phase's code will drive.

2. **Should the changelog generator (D-12/999.5) include ALL conventional-
   commit types in the range (docs/chore/test/ci/refactor/style, which are
   `Bump::None` and don't affect versioning) or only the version-affecting
   ones (feat/fix/perf/breaking)?**
   - What we know: the hand-written `CHANGELOG.md` entries for past
     releases include narrative `### Fixed`/`### Added`/`### Changed`
     sections that clearly draw from more than just feat/fix commits (e.g.
     a `docs:`-typed CONTRIBUTING.md fix appears under `### Changed` in the
     2.1.0 entry).
   - What's unclear: whether an *automated*, mechanical version of this
     (grouped raw commit subjects, not hand-curated narrative) should
     filter down to only feat/fix/perf (a shorter, more signal-dense
     changelog) or include everything (closer to the historical hand-
     written style, but noisier and including internal chores).
   - Recommendation: default to feat→Added, fix/perf→Fixed, breaking→a
     `### Breaking` heading, and omit docs/chore/test/ci/refactor/style
     entirely (matching the version-affecting subset `classify_range_bump`
     itself cares about) — this is a reasonable, defensible default the
     planner can lock in as a decision rather than leaving genuinely open;
     flag it for one round of operator confirmation given it's a visible,
     user-facing artifact choice.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | Every step | ✓ | 2.55.0 | — |
| `cargo` | Version-bump write, `cargo info` check, `cargo publish` | ✓ | 1.97.1 (pinned) | — |
| `gh` | Operator's manual `main` PR step only (not newly invoked by this phase's code) | ✓ | 2.96.0 | — |
| `ssh-add`/ssh-agent with the maintainer's `devflow.releaseSigningKey` loaded | Signed tag step (D-10) | ✗ in this sandbox (`ssh-add -l` → "The agent has no identities") | — | None at automation time — this is a genuine human-environment precondition per D-10's own reasoning ("the environment is expected to already be configured correctly"); the executor should surface `git tag -s`'s real failure if the key isn't loaded, not attempt to detect this ahead of time |
| `CARGO_REGISTRY_TOKEN` / `cargo login` credentials | `cargo publish` step | Not verified in this sandbox (no publish attempted) | — | None — same class as the signing key; a genuine operator-environment precondition, not something to preflight-predict (consistent with D-10's philosophy generally, even though D-10 itself is scoped to signing specifically) |
| GitHub ruleset bypass for `develop` direct pushes | D-01/D-08's push steps | Not verifiable from this checkout (a GitHub-side setting, operator's own responsibility, out of this phase's scope) | — | None — explicitly the operator's out-of-band task per D-01 |

**Missing dependencies with no fallback:**
- A loaded, correctly-keyed ssh-agent identity for `devflow.releaseSigningKey` — required at actual tag-signing time, not at build/test time. Not this phase's job to provide; document as a precondition.
- `cargo` publish credentials — same category.
- The GitHub ruleset bypass — same category, explicitly out of scope per D-01.

**Missing dependencies with fallback:** none — every item above is a
genuine "must exist in the real release environment" precondition with no
code-level workaround, by design (this is the D-10 philosophy applied
consistently: DevFlow does the real operation and reports the real result,
rather than trying to predict or route around an unconfigured environment).

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (workspace), standard Rust test harness — no separate framework |
| Config file | none — `Cargo.toml`'s `[workspace]`/per-crate `[dev-dependencies]` is the only config |
| Quick run command | `cargo test --workspace <new_module>::` (targeted) or `devflow test` (runs the same fmt+clippy+test triad CI/pre-push require) |
| Full suite command | `devflow test` / `scripts/check.sh all` / `scripts/check-in-container.sh all` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| 999.52 | `devflow sync` refuses on dirty tree | unit | `cargo test -p devflow-core sync::tests::refuses_on_dirty_tree -- --exact` | ❌ Wave 0 (new `sync.rs`) |
| 999.52 | `devflow sync` refuses on non-`develop` branch | unit | `cargo test -p devflow-core sync::tests::refuses_off_develop -- --exact` | ❌ Wave 0 |
| 999.52 | `devflow sync` short-circuits when `origin/main` already an ancestor | unit (bare-remote fixture) | `cargo test -p devflow-core sync::tests::noop_when_already_synced -- --exact` | ❌ Wave 0 |
| 999.52 | Tree-identity mismatch aborts and leaves `develop` untouched, no push | unit (bare-remote fixture) | `cargo test -p devflow-core sync::tests::aborts_on_tree_mismatch -- --exact` | ❌ Wave 0 |
| 999.52 | Successful sync pushes to origin (D-08) | unit (bare-remote fixture) | `cargo test -p devflow-core sync::tests::pushes_on_success -- --exact` | ❌ Wave 0 |
| 999.5 | Changelog entry groups commits by type (feat→Added etc.), replaces hardcoded string | unit | `cargo test -p devflow-core version::tests::changelog_content_groups_by_type -- --exact` | ❌ Wave 0 |
| 999.5 | Empty range (no version-affecting commits) still produces a sensible entry, not an empty one | unit | `cargo test -p devflow-core version::tests::changelog_content_handles_empty_range -- --exact` | ❌ Wave 0 |
| 999.25 | Version-bump commit pushes to `develop` (D-01), fast-forward only, no `--force` | unit (bare-remote fixture) | `cargo test -p devflow-core git::tests::version_bump_pushes_develop -- --exact` | ❌ Wave 0 |
| 999.25 | Idempotent resume: develop already at/ahead of computed version → push step skipped | unit | `cargo test -p devflow-core git::tests::skips_push_when_already_ahead -- --exact` | ❌ Wave 0 |
| 999.25 | Tag step: existing annotated+reachable+pushed tag → skip (D-06) | unit | `cargo test -p devflow-core git::tests::skips_tag_when_already_released -- --exact` | ❌ Wave 0 |
| 999.25 | Tag step: existing lightweight tag with the same name → does NOT silently skip (Pitfall 2) | unit | `cargo test -p devflow-core git::tests::stray_lightweight_tag_is_not_treated_as_released -- --exact` | ❌ Wave 0 |
| 999.25 | `cargo info` check: exit 0 → already-published; `could not find` in stderr → not-published; other → ambiguous error | unit (mocked via a fake `cargo` on PATH, or an integration test hitting the real registry for a known-old version like `serde@1.0.1`) | `cargo test -p devflow-core git::tests::publish_check_classifies_exit_codes -- --exact` | ❌ Wave 0 |
| 999.25 | Publish order still matches `publish_order`'s existing topo-sort (no recomputation) | unit (existing) | `cargo test -p devflow-core git::tests::publish_order_derives_core_before_cli_from_a_fixture_workspace -- --exact` | ✓ already exists |
| 999.25 | `--yes-release` is required per-invocation, never settable via config/env (mirrors `--yes-ship`) | unit/CLI | new `crates/devflow-cli/tests/release_execute.rs`, following `release_check.rs`'s existing shape | ❌ Wave 0 |
| 999.25 | Fail-fast, no rollback: a mid-sequence failure (e.g. publish of `devflow-core` succeeds, `devflow` fails) leaves prior steps landed | integration | `cargo test -p devflow-cli release_execute::tests::partial_failure_leaves_prior_steps_landed -- --exact` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p devflow-core <module>::` (targeted, fast)
- **Per wave merge:** `devflow test` (full fmt+clippy+test triad, matches CI/pre-push)
- **Phase gate:** Full suite green before `/gsd-verify-work`; additionally, per this project's own precedent (Phase 20's UAT), the actual signed-tag and `cargo publish` code paths should get a live UAT-style operator confirmation against a real (non-production) registry/repo before being trusted unattended, since no sandbox in this research session had real signing keys or publish credentials to exercise them end-to-end.

### Wave 0 Gaps
- [ ] `crates/devflow-core/src/sync.rs` — new module, no existing file
- [ ] `crates/devflow-cli/tests/release_execute.rs` — new integration test file, following `crates/devflow-cli/tests/release_check.rs`'s existing shape (already present, read for precedent)
- [ ] Bare-remote git fixture helper (extend `git.rs`'s existing `init_repo()`/`flow()` pattern with a second bare-repo remote) — needed by every push-mutating test above; does not exist today since no function has ever pushed in production
- [ ] A fake/stub `cargo` shim for hermetic unit testing of the exit-code classification (Pitfall 3), OR accept a small number of tests that hit the real crates.io registry for known-immutable old versions (e.g. `serde@1.0.1`, guaranteed never to be un-published) — pick one approach and note the tradeoff (hermeticity vs. testing the real tool's actual behavior) in the plan

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-------------------|
| V2 Authentication | yes | Delegated entirely to git/gh/cargo's own credential stores (SSH keys, `gh auth`, `~/.cargo/credentials.toml`) — this phase must never read, log, or handle a raw credential itself |
| V4 Access Control | yes | The GitHub ruleset bypass (D-01) and `main`'s continued PR gate (D-02) ARE the access-control boundary; this phase must not weaken either — no code path should ever push to `main` directly or merge the release PR programmatically |
| V6 Cryptography | yes | Tag signing (SSH-format, per `.gitconfig`) — never hand-rolled, always the real `git tag -s` invocation with the explicit `-c user.signingkey=` override (D-10); never derive, generate, or manage key material in this phase's code |
| V7 Error Handling / Information Disclosure | yes | Commit subjects fed into changelog content and gate-context error strings are attacker/contributor-influenced text (same class as `T-17-13`/`T-25-52`'s existing redaction discipline in `preflight.rs`) — truncate/redact before surfacing in logs or gate context, following the existing `truncate_reason` precedent |
| V9 Communication (transport) | yes (delegated) | All network communication (git push/fetch, crates.io queries, `cargo publish`) goes through git/cargo's own TLS handling — this phase adds no direct network code |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|----------------------|
| A stray/leftover lightweight tag silently treated as "already released" (Pitfall 2), causing the executor to skip creating the real signed tag | Tampering (of release provenance) / Repudiation (no real signed tag exists for a version claimed released) | The three-part existence check (annotated + `git tag -v` passes + reachable-and-pushed) in Pattern 3/Pitfall 2 — never treat bare `git tag -l` existence as sufficient |
| Command injection via a crate/branch name interpolated into a shelled-out `git`/`cargo` command | Tampering | This project already avoids shell interpolation everywhere — every existing `git.rs` function uses `Command::new("git").args([...])` (argv array, not a shell string), never `sh -c "git ... {name}"`. New code must follow the identical pattern; the one existing exception (`docs_update`'s `Command::new("sh").arg("-c")`) takes no untrusted input and should not be used as precedent here |
| Force-pushing `develop`/`main` inadvertently overwriting others' work | Tampering / Denial of Service | No step in this design ever uses `--force`/`--force-with-lease` (see Alternatives Considered) — a rejected non-fast-forward push is a hard error, never auto-resolved |
| Publishing to crates.io with a stale/wrong `Cargo.lock`, or in the wrong order, breaking `devflow`'s path-to-registry dependency resolution | Tampering (of the published artifact graph) | Reuse `publish_order` unchanged (Don't Hand-Roll); the D-04 pre-publish `cargo info` check additionally prevents a duplicate/conflicting publish attempt |
| Commit-message-derived changelog/error text used for a log-injection or terminal-escape attack (a contributor crafts a malicious commit subject) | Tampering / Information Disclosure | Same truncation/redaction discipline already applied to major-bump-gate reasons in `preflight.rs` (`truncate_reason`) — apply identically to changelog content and any gate context this phase adds |

## Sources

### Primary (HIGH confidence — direct codebase inspection, this checkout)
- `crates/devflow-cli/src/main.rs` (Command::Release, `--yes-ship` pattern, CliError) — read in full relevant ranges
- `crates/devflow-core/src/git.rs` (GitFlow, publish_order, tag/push/release_start/release_finish, SigningViability, test fixtures) — read in full
- `crates/devflow-core/src/version.rs` (compute_version, classify_range_bump, release_range_start, Bump enum) — read in full relevant ranges
- `crates/devflow-core/src/hooks.rs` (hooks_after_ship, version_bump, changelog_append, merge_feature) — read in full
- `crates/devflow-core/src/ship.rs` (prepend_changelog) — read
- `crates/devflow-cli/src/preflight.rs` (gh call sites, major-bump gate, signing check) — read relevant ranges
- `crates/devflow-cli/src/commands.rs` (release_check, check_signing, check_publish_order) — read relevant ranges
- `scripts/sync-main-to-develop.sh` — read in full
- `CONTRIBUTING.md` §§ Release signing, Cutting a Release — read in full
- `CHANGELOG.md` (2.1.0, 2.0.0 entries) — read, confirms "release cut is deliberately manual" as of HEAD
- `.planning/ROADMAP.md` §§ Phase 999.5, 999.52, 999.54, 999.55 — read
- `.planning/phases/26-release-cut-automation/999.25-BACKLOG-DOSSIER.md`, `999.5-BACKLOG-DOSSIER.md` — read in full
- Live command output from this exact checkout: `cargo info devflow-core@{1.8.0,0.0.1}`, `cargo info serde@{1.0.1,999.999.999}`, `git cat-file -t` on all local `v*` tags, `git worktree list`, `cargo --version`/`git --version`/`gh --version`, `.gitconfig` contents

### Secondary (MEDIUM confidence)
- [`cargo info` command reference](https://doc.rust-lang.org/cargo/commands/cargo-info.html) — official Cargo Book, cross-referenced with [InfoWorld's Rust 1.82 coverage](https://www.infoworld.com/article/3574858/rust-1-82-brings-cargo-info-subcommand.html) confirming stabilization version

### Tertiary (LOW confidence)
- none used for a load-bearing claim in this document

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependency; every tool version verified live in this environment
- Architecture: HIGH — every existing function/pattern cited was read directly from source in this checkout, including confirming zero production callers for `GitFlow::push`/`delete_remote_branch`/`release_start`/`release_finish`
- Pitfalls: HIGH for Pitfalls 1, 3, 4 (directly verified by source read + live command output); MEDIUM for Pitfall 2's causal attribution (the collision *mechanism* is HIGH-confidence verified; whether it has *already occurred via that specific mechanism* in this repo is explicitly flagged as Open Question 1 / Assumption A2, not overclaimed)

**Research date:** 2026-07-29
**Valid until:** 30 days (stable internal-automation domain; the one time-sensitive external fact — `cargo info`'s stabilization version — is a fixed historical fact, not something that will go stale)
