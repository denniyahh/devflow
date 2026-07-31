# Phase 29: Release-Cut Executor — Observe, Then Act Within the Repo's Rules — Research

**Researched:** 2026-07-31
**Domain:** Git/GitHub release automation (branch protection, PR merges, signed tags) + crates.io publish orchestration, in Rust, on top of an existing DevFlow git substrate
**Confidence:** HIGH for the mechanics (all load-bearing facts verified live this session against the real repo/registry); MEDIUM for exact task/module shape (Claude's discretion, no CONTEXT.md); LOW/flagged where noted (the one open recommendation from ROADMAP awaiting operator ruling)

## Summary

Phase 29 rebuilds `devflow release` as an executor, not a checker, but the design that failed in Phase 26 was not the *steps* — it was treating progress as something to record. This research confirms, against live sources, that every one of Phase 26's five open Criticals traces to that one lifecycle defect, and that removing it (derived state, computed never asserted) dissolves three of them by construction and turns the other two into ordinary code changes already substantially unblocked by prior phases.

The three units map cleanly onto three architectural tiers of trust: **29a (observer)** reads two external oracles — git remote refs and the crates.io HTTP API — and asserts nothing; it is the only unit that is straightforwardly unit-testable, because pure observation is what tests are good at. **29b (recoverable actions)** drives GitHub's PR machinery (`gh pr create` / `gh pr merge --auto <method>`), which this session confirmed live is fully automatable today: both `develop` and `main` require 0 approvals, `auto_merge` is enabled repo-wide, and the four required checks (`Test`, `Clippy`, `Format`, `Build + test in devcontainer`) are named exactly as ROADMAP.md recorded. **29c (commit point)** is two irreversible operations — `git tag -s` and `cargo publish` — that must never be predicted, only attempted, with their real exit codes read back. This session reproduced the exact "signed tag never created, crates published anyway" scenario ROADMAP.md describes as motivating evidence: `git ls-remote --tags origin` shows `v2.1.0` as the newest tag, while `crates.io`'s API confirms both `devflow-core` and `devflow` are live at `2.2.0` — a live, reproducible instance of "unreachable ≠ absent" and of why observation (29a) has value independent of speed.

**Primary recommendation:** Build 29a first as pure observation over `git ls-remote`/`git ls-remote --tags`/the crates.io `/api/v1/crates/{name}/{version}` JSON endpoint (never the CDN-cached sparse index, which the Common Pitfalls section explains). Build 29b's PR flow on `gh pr create` + `gh pr merge --auto <method>` with the method always passed explicitly — GitHub's own auto-merge polls checks, so DevFlow needs no polling loop of its own, and re-running 29a *is* checking whether it landed. Build 29c's tag/publish steps as literal, undecorated invocations of the exact commands CONTRIBUTING.md already documents, with no viability prediction anywhere, consistent with D-10.

## User Constraints (from ROADMAP.md — no 29-CONTEXT.md exists for this phase)

> This phase's Define stage deliberately did not run `/gsd-discuss-phase`; the roadmap's Phase 29 section (`.planning/ROADMAP.md:2033-2207`) is the authoritative context source and functions as this phase's locked-decision record.

### Locked Decisions (operator-stated, do not re-litigate)

- **The design rule (D-10 generalized), governs every unit:** DevFlow discovers the repo's rules, advances as far as they permit, and stops at the first hard gate with an accurate report of where it stopped and why. Never predicts a gate, never routes around one, never treats stopping at one as failure. Layering: action set fixed/enumerable; route selection adaptive (informed by discovered facts); outcome authoritative only from performing the operation and reading the real result.
- **`feature/phase-26` is reference material only.** Not rebased, not carried forward. Read for failure modes, not copied for design.
- **NO OPERATOR-PRESENCE REQUIREMENT.** The executor must not require a human at the keyboard and must not refuse to run unattended. Authorization is a one-time mandate (the `--yes-ship` precedent), not a live-presence check. A human-presence precondition would itself be a self-imposed gate the design rule forbids.
- **Authorization is a mandate, not a presence check.** The operator grants intent once (a flag); thereafter the executor proceeds as far as the repo's rules and environment permit.
- **D-10 carried unchanged.** No signing-viability predictor, ever, anywhere in this phase. The tag step runs the real `git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" tag -s vX.Y.Z <commit> -m "vX.Y.Z"` and reports git's own exit code / `git tag -v` result.
- **D-05 carried and strengthened.** Fail-fast, no automatic rollback — trivially safe once every step is independently re-runnable (which derived state makes true by construction).
- **D-06 superseded** by derived state (Phase 26's D-06 specified ledger-based resume/idempotency; there is no ledger in Phase 29's design).
- **Do NOT add a GitHub ruleset bypass for this phase.** Verified live this session (`gh api repos/denniyahh/devflow/rulesets`): required approvals are 0 on both `develop` and `main`. The PR route needs no bypass. Do not hardcode "squash on main, merge on develop" — discover allowed methods, apply a fixed internal policy, refuse loudly if the preferred method isn't in the allowed set.
- **Unit split by reversibility, each independently shippable:** 29a (observer, build first) → 29b (recoverable actions) → 29c (commit point). A stall in 29c must still leave 29a and 29b delivered and useful.
- **State is derived, never recorded.** No progress ledger of any kind. Every irreversible step has an authoritative external oracle (remote ref SHA, crates.io API). The only thing that cannot be observed is operator authorization, so a minimal persisted record may survive for *authorization only*, never for *progress*. Pinning that boundary is explicitly named in ROADMAP.md as a discuss-phase task — since no discuss-phase ran, **this is the single most important open design question for the planner to resolve** (see Open Questions).
- **Every observe step needs an explicit `unreachable ≠ absent` arm that refuses rather than proceeds.** A network partition must never be reported as "not done."

### Claude's Discretion (research options, recommend — not fixed by the roadmap)

- Exact CLI surface shape for `devflow release status` / the recoverable-action verbs / the commit-point verbs (new `Command::Release{...}` fields vs. new subcommands).
- Where new code lives in `git.rs` / a new module vs. reusing `GitFlow`.
- Exact shape of the crates.io HTTP call (shell out to `curl` vs. add a minimal HTTP crate) — this research recommends shelling out, see Standard Stack.
- Retry/backoff shape (if any) for crates.io queries and `gh pr checks` polling — this research finds GitHub's native `--auto` merge makes DevFlow-side polling largely unnecessary (see Architecture Patterns).
- Whether `devflow sync` (999.52, referenced but not itself renumbered into 29a-c) is folded into 29b or built as a fourth, later unit — Phase 26's D-07 (standalone + internal reuse) is sound design and not contradicted by anything in Phase 29's redesign; carrying it forward as a recommendation, not a lock.

### Deferred Ideas (OUT OF SCOPE — do not build)

- Any GitHub ruleset bypass configuration (explicitly retired, see D-10 generalized discussion above).
- `check_ssh_signing_viability`'s existing classification bug (999.27, already shipped separately) — untouched.
- `devflow parallel`'s future (removal vs. repurposing) — unrelated, deferred to its own phase.
- Claude reviewing the `develop → main` release PR instead of a human — future direction, not this phase; the PR route this phase builds is fully automatable by rule discovery, not by a review capability that doesn't exist yet.

## Phase Units (no REQ-IDs — tracked by unit identifier, per Phases 21/22/26/27/28 convention)

| Unit | Description | Research Support |
|------|-------------|-------------------|
| 29a | Read-only `devflow release status` — six yes/no questions answered by observation: version bumped, changelog written, release PR merged, signed tag present on remote, sync merged, both crates published | See Architecture Patterns (Oracle table), Common Pitfalls (index lag, unreachable≠absent), Code Examples |
| 29b | Recoverable actions: version bump (two `Cargo.toml` locations), changelog, release PR to `main`, sync PR back to `develop` | See Architecture Patterns (PR automation, merge-method policy table), CONTRIBUTING.md-sourced sync script port |
| 29c | Commit point: signed tag, then `cargo publish` for `devflow-core` then `devflow`, in order | See Architecture Patterns (tag/publish sequencing), Common Pitfalls (namespace collision, index lag), Don't Hand-Roll |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Observing remote git ref/tag state | DevFlow CLI process (local, network I/O to `origin`) | — | `git ls-remote` reads directly from the remote without a local clone/fetch mutation; keeps 29a read-only by construction |
| Observing crates.io publish state | DevFlow CLI process (HTTP client, network I/O to crates.io) | — | The crates.io API is the authoritative source of truth; no local cache is trustworthy (index lag, see Common Pitfalls) |
| Version bump / changelog write | DevFlow CLI process (local git ops) + GitHub (PR merge) | — | Local computation (`version::compute_version`, already exists), landed via PR because `develop` is protected |
| Release PR / sync PR / merge orchestration | GitHub (branch protection, checks, merge) | DevFlow CLI process (`gh` invocations) | GitHub owns rule enforcement and check execution; DevFlow only discovers and requests, never simulates GitHub's ruleset engine |
| Signed tag creation | Local git + the operator's/CI's configured signing key material | GitHub (pre-push hook enforcement, "Verified" badge) | Git is the sole authority on whether a signature is valid; DevFlow must never re-implement that judgment (D-10) |
| crates.io publish | `cargo` (local process invoking the registry API) | crates.io (registry acceptance/rejection) | `cargo publish`'s own duplicate-version rejection is itself part of the oracle — a second implementation of "already published" is exactly what D-04's crates.io pre-check exists to avoid needing |

## Standard Stack

### Core

| Tool/Library | Version (verified live) | Purpose | Why Standard |
|---|---|---|---|
| `git` CLI (via `devflow_core::git::git_command`/`hermetic_command`) | 2.55.0 (host), scrub table already covers this version | All git operations: `ls-remote`, `tag -s`, `push`, `fetch` | Already the only sanctioned way to invoke git in this codebase (Phase 27 hermeticity substrate) — every new call in this phase MUST go through `git_command`/`hermetic_command`, never `Command::new("git")` directly |
| `gh` CLI | 2.96.0 (host, authenticated) | `gh pr create`, `gh pr merge --auto <method>`, `gh api repos/.../rulesets`, `gh pr view --json` | Already the only sanctioned way DevFlow talks to GitHub (2 existing call sites: `gh auth status` in preflight.rs, `gh --version` doctor check in commands.rs) — Phase 26's D-02 discussion confirms no existing `gh pr create`/`gh pr merge` call site, so this phase adds the first ones |
| `cargo publish` (via `Command::new("cargo")`) | 1.97.1 (host) | Publishing `devflow-core` then `devflow` | No existing call site; `publish_order` (`git.rs:580-`) already computes the sequence and must be reused, not recomputed |
| `curl` (shell-out, via a new hermetic `Command`) | 8.21.0 (host) | crates.io API query for 29a's publish-state oracle and 29c's pre-publish idempotency check | See rationale below — matches this codebase's established convention of shelling to external tools rather than adding Rust HTTP-client dependencies |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `devflow_core::version::compute_version` / `read_version` / `write_version` | already in-tree | Version arithmetic and file I/O, unchanged | 29b's version-bump step must call these, not reimplement semver logic |
| `devflow_core::version::render_changelog_body` / `changelog_sections` | already in-tree | Conventional-commit-classified changelog content (Phase 25/D-12's already-delivered content source) | 29b's changelog step; this is the "delivered and sound" piece Phase 26 explicitly built cleanly (`999.5`) — reuse verbatim, it was not on the unmerged branch's list of defective pieces |
| `devflow_core::git::publish_order` | already in-tree | Topologically-sorted crates.io publish order from the workspace manifest | 29c's publish step MUST call this, never hardcode "core then cli" as a literal (D-04 already specifies "reuse, don't recompute") |
| `devflow_core::git::check_signing_viability` | already in-tree | **Explicitly excluded from this phase's execute path** (D-10) | Reference only — do not call it from any new code; ROADMAP.md's Phase 26 retro states `check_signing` is "deliberately excluded from the execute pre-gate" |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Shelling to `curl` for the crates.io query | Add `ureq` (minimal, sync, no-async-runtime HTTP client crate) as a new `devflow-core` dependency | `ureq` is a smaller, more idiomatic Rust surface (typed responses, no shell-quoting/parsing) but is a **new external dependency** in a codebase that has deliberately avoided pulling in libraries for external interaction even where a crate would be more idiomatic (this project hand-rolls TOML member-parsing in `git.rs` rather than add a TOML crate; shells to `ssh-keygen`/`gpg`/`gh`/`cargo` rather than binding to their libraries). Recommend `curl` shell-out as the default; if the planner or a future reviewer prefers `ureq`, it MUST go through the Package Legitimacy Audit gate below before being added — this research did not run that gate because it does not recommend adding the dependency |
| `gh pr merge --auto <method>` (GitHub-native auto-merge, confirmed enabled) | A DevFlow-side polling loop (`gh pr checks --watch` in a retry loop) | Auto-merge is a repo setting (`allow_auto_merge: true`, verified live) that makes GitHub itself the thing waiting on checks — no DevFlow-side timeout/backoff code needed. A polling loop would be a second implementation of "are checks done" running alongside GitHub's own, and re-running 29a is the correct way to check whether the auto-merge eventually landed (derived state, not a wait) |
| The crates.io `/api/v1/crates/{name}/{version}` JSON endpoint (DB-backed, authoritative) | The sparse index (`index.crates.io`) or `cargo info`/`cargo search` | The sparse index is CDN-cached and can lag behind a publish by observed seconds (this is the literal mechanism behind the "waiting for devflow-core 2.2.0 to be available" message `cargo publish` itself printed during the real v2.2.0 cut) — using it for the observer's oracle would reintroduce the exact ambiguity D-10-style prediction exists to avoid. The `/api/v1` endpoint returned instantly and correctly for both a real (`devflow-core@2.2.0` → 200) and fabricated (`devflow-core@999.999.999` → 404) version, verified live this session |

**Installation:** No new dependencies required if the `curl` shell-out recommendation is followed — `git`, `gh`, `cargo`, `curl` are all already-assumed environment tools (see Environment Availability).

**Version verification:** All four core tools verified live against this exact host during research (`git --version` → 2.55.0, `gh --version` → 2.96.0, `cargo --version` → 1.97.1, `curl --version` → 8.21.0). No Rust crate versions to verify — no new crate dependencies recommended.

## Package Legitimacy Audit

**No new external packages required.** This phase's design (shelling to `curl`/`gh`/`cargo`/`git`, all already present in this codebase's toolchain) adds zero new `Cargo.toml` dependencies. The Package Legitimacy Gate protocol was not run because there is nothing to run it against. If a future planner or reviewer chooses the `ureq` alternative noted above, the gate must be run at that time — `gsd-tools query package-legitimacy check --ecosystem npm ureq` (or the crates.io-appropriate check) before it is added.

**Packages removed due to [SLOP] verdict:** none.
**Packages flagged as suspicious [SUS]:** none.

## Architecture Patterns

### System Architecture Diagram

```
                         devflow release status <version>          (29a, read-only)
                                    │
              ┌─────────────────────┼─────────────────────────────────┐
              ▼                     ▼                                 ▼
     git ls-remote origin    git ls-remote --tags origin      curl crates.io API
     (develop/main SHAs,     (vX.Y.Z present? annotated?      (/api/v1/crates/{name}/{ver}
      PR-merge ancestry)      peeled — signed?)                200=published, 404=absent)
              │                     │                                 │
              └─────────┬───────────┴─────────────────┬───────────────┘
                         ▼                             ▼
                 unreachable? → refuse         absent? → report "not done"
                 (network/DNS/auth failure,     present? → report "done"
                  distinct from "absent")
                         │
                         ▼
              devflow release status prints six booleans
              (version bumped / changelog / PR merged / tag present / sync merged / both published)
                         │
       ┌─────────────────┴──────────────────────────────────────┐
       ▼                                                         ▼
  devflow release bump-and-pr (29b)                    devflow release commit (29c)
  ─────────────────────────────                        ───────────────────────────
  1. compute_version() [existing]                       Preconditions gated on 29a's
  2. write_version() x2 locations [existing]             observation, not a flag:
  3. render_changelog_body() [existing]                  - release PR observed merged
  4. commit_path() x2 [existing GitFlow]                 - tag NOT already present
  5. gh pr create → develop                             1. git -c user.signingkey=... \
  6. gh pr merge --auto squash (discovered method)          tag -s vX.Y.Z <commit>
  7. gh pr create → main (release PR)                       [real command, real exit code —
  8. gh pr merge --auto squash (discovered method)           pre-push hook + GitHub "Verified"
  9. devflow sync: git merge -X ours origin/main             badge are the real oracle, D-10]
     [existing script logic, ported]                      2. git push origin vX.Y.Z
  10. gh pr create → develop (sync PR)                        [pre-push hook enforces the
  11. gh pr merge --auto merge (NOT squash —                  maintainer-key fingerprint —
      ancestry-preserving, hard constraint,                   defense in depth, D-10-consistent]
      see Common Pitfalls)                                 3. cargo publish -p devflow-core
                                                                [publish_order()[0]]
                                                             4. curl crates.io API poll for
                                                                devflow-core@vX.Y.Z (handles
                                                                index lag — see Pitfalls)
                                                             5. cargo publish -p devflow
                                                                [publish_order()[1]]
```

### Recommended Project Structure

```
crates/devflow-core/src/
├── git.rs                  # extend: signed-tag-present-and-verified check,
│                            #   ls-remote wrappers, gh-ruleset-discovery wrapper
├── release_observe.rs       # NEW (29a): six-question observer, pure logic +
│                            #   thin I/O wrappers, no local state file
├── release_publish.rs       # NEW (29c): cargo publish invocation + crates.io
│                            #   HTTP query, index-lag-aware retry
crates/devflow-cli/src/
├── commands.rs              # extend: `release_status`, alongside existing
│                            #   `release_check` (rename/namespace to avoid
│                            #   confusion between --check preflight and the
│                            #   new status/execute verbs — Claude's discretion)
├── main.rs                  # extend `Command::Release` or add new variants
│                            #   for status / bump-and-pr / commit
```

### Pattern 1: Oracle-per-question, no aggregation into stored state

**What:** Each of the six observations in 29a is an independent function returning a three-way result (`Present`, `Absent`, `Unreachable`) — never a boolean, never persisted, never aggregated into a struct that itself becomes a source of truth.
**When to use:** Every observation in 29a, and every "is this step already done" check inside 29b/29c before attempting an action.
**Example (crates.io oracle, informed by this session's live verification):**
```rust
// Source: this session's live curl verification against
// https://crates.io/api/v1/crates/devflow-core/2.2.0 (200) and
// https://crates.io/api/v1/crates/devflow-core/999.999.999 (404)
pub enum PublishState {
    Published,
    NotPublished,
    Unreachable { reason: String }, // network/DNS/timeout — NEVER collapsed into NotPublished
}

fn crate_version_published(name: &str, version: &str) -> PublishState {
    let url = format!("https://crates.io/api/v1/crates/{name}/{version}");
    let output = Command::new("curl")
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}",
               "-A", "devflow-release-executor (contact: <repo-url>)", &url])
        .output();
    match output {
        Err(e) => PublishState::Unreachable { reason: e.to_string() },
        Ok(out) if !out.status.success() => PublishState::Unreachable {
            reason: "curl exited non-zero".into(),
        },
        Ok(out) => match String::from_utf8_lossy(&out.stdout).trim() {
            "200" => PublishState::Published,
            "404" => PublishState::NotPublished,
            other => PublishState::Unreachable {
                reason: format!("unexpected HTTP status {other}"),
            },
        },
    }
}
```

### Pattern 2: Explicit merge-method policy table, discovered then applied

**What:** Query `gh api repos/{owner}/{repo}/rulesets` for `allowed_merge_methods` per target branch, then apply a **fixed internal policy keyed to PR intent** (not branch name alone) — because this repo's own documented incident (the v2.0.0 sync failure, CONTRIBUTING.md § "Cutting a Release" step 6) proves "develop allows merge or squash" is not enough information; the *sync-back* PR specifically requires `merge` to preserve ancestry, while an ordinary version-bump PR to `develop` can safely use `squash`.
**When to use:** Every `gh pr merge` call in 29b.
**Example:**
```rust
// Source: CONTRIBUTING.md § "Cutting a Release" step 6 (this repo's own
// documented incident: an unspecified-method auto-merge on the sync PR
// defaulted to squash on 2026-07-27, destroying the ancestry link a
// second time) + live-verified ruleset data (gh api .../rulesets, this
// session).
enum PrIntent { VersionBump, ReleaseCut, SyncBack }

fn required_merge_method(intent: PrIntent, allowed: &[&str]) -> Result<&'static str, String> {
    let preferred = match intent {
        PrIntent::VersionBump => "squash",
        PrIntent::ReleaseCut => "squash",   // main only allows squash anyway
        PrIntent::SyncBack => "merge",       // MUST preserve ancestry — never squash
    };
    if allowed.contains(&preferred) {
        Ok(preferred)
    } else {
        Err(format!(
            "required merge method '{preferred}' for {intent:?} is not in this repo's \
             allowed set {allowed:?} — refusing rather than silently substituting"
        ))
    }
}
```
`gh pr merge --auto --squash` or `gh pr merge --auto --merge` is then invoked with the method as an **explicit flag**, never bare `--auto` (which defers to GitHub's default, itself squash — the exact mechanism of the documented incident).

### Pattern 3: Signed-tag presence is a three-part check, not a ref lookup

**What:** "Is the signed tag present on the remote" is not answered by `git ls-remote --tags` alone. A lightweight tag has no peeled (`^{}`) entry; an annotated-but-unsigned tag does; a signed tag additionally requires `git verify-tag` (needs the signer's public key available) or a `git cat-file tag <tag>` body inspection for an SSH/PGP signature block, run against a **fetched** tag object, since `ls-remote` alone gives you the SHA but not the object contents.
**When to use:** 29a's "signed tag present" oracle, and 29c's pre-tag idempotency check (the IN-01 collision case below).
**Example (verified live against this repo's real tags):**
```bash
# Verified live this session:
$ git ls-remote --tags origin
...
86cf95151ff88574e69cd8342757dbb87fabffda  refs/tags/v2.1.0
2a82861ab39b62d8d4df93b73f1cad918fe528b9  refs/tags/v2.1.0^{}     # peeled entry present → annotated
# (no v2.2.0 entries at all — the tag was genuinely never created,
#  reproducing ROADMAP.md's "motivating evidence" exactly)
```
A peeled entry proves "annotated," not "signed." Signedness requires fetching the tag object (`git fetch origin refs/tags/vX.Y.Z` or `git cat-file`) and checking for a signature block, or running `git verify-tag` against a repo that has the maintainer's public key available (the CI/local environment DevFlow runs in may or may not have it — this is itself an `Unreachable`-shaped case, not an `Absent` one, if the key can't be resolved to verify).

### Anti-Patterns to Avoid

- **A local progress file of any kind for 29a/29b/29c's own step completion.** This is the exact CR-05/CR-02 mechanism from Phase 26 — "an in-flight ledger permanently bricks the release path" and "`CompletedWithoutPublish` exits 0, marks the ledger Complete." Nothing in this phase's design may write a file whose *contents*, once read back, are trusted over a fresh observation.
- **Bare `gh pr merge --auto`** with no method flag — reproduces the documented 2026-07-27 sync-ancestry incident.
- **Treating an HTTP timeout/DNS failure as "not published."** This collapses `Unreachable` into `Absent`, which is precisely the ban the roadmap states explicitly ("every observe step needs an explicit unreachable ≠ absent arm that refuses rather than proceeds").
- **Recomputing `publish_order` or the signed-tag command form from scratch.** Both already exist (`git.rs:580-`, CONTRIBUTING.md's literal command block) — a second implementation is a second thing that can drift from the first, which is the exact bug class D-10 exists to prevent for signing and D-04 exists to prevent for publish ordering.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| "Are this PR's required checks green yet?" | A polling loop with backoff calling `gh pr checks` | `gh pr merge --auto <method>` (repo-native `allow_auto_merge`, verified `true` live) | GitHub already implements exactly this wait; a DevFlow-side loop is a second implementation with its own timeout-tuning problem, and under derived state, re-observing later is free anyway |
| "Which merge methods does this branch allow?" | Hardcoded `if branch == "main" { squash } else { merge }` | `gh api repos/{owner}/{repo}/rulesets`, `allowed_merge_methods` field | Explicitly what the roadmap forbids — "a copy of live config that goes stale, the same failure mode as CONTRIBUTING.md drifting" |
| "Is devflow-core@X.Y.Z live on the registry?" | Parsing `cargo info`'s stderr for a substring (Phase 26's WR-04, already flagged fragile — the version could diverge from what the manifest says) | `curl` the crates.io `/api/v1/crates/{name}/{version}` JSON endpoint and check HTTP status | Structured, versioned API response vs. parsing human-readable CLI text not designed as a machine contract |
| "Is this git tag properly signed by the release key?" | A viability predictor (ssh-agent introspection, key-file existence checks) applied to the tag step | The real `git tag -s` + `git tag -v` / the existing `scripts/hooks/pre-push` fingerprint check | D-10, verbatim, twice-stated in ROADMAP.md and 26-CONTEXT.md — this project has twice deleted signing predictors (999.50, 999.54) for drifting from git's real behavior |
| Sync-back merge logic (`-X ours` + tree-identity verification) | A new from-scratch implementation | Port `scripts/sync-main-to-develop.sh` almost verbatim (already proven; Phase 26's D-07/D-09 already decided this, uncontradicted by the redesign) | The script's every check (clean tree, correct branch, fetch-first, already-ancestor short-circuit, tree-identity verification) is production-proven; this phase's job is to move it from bash+direct-push to Rust+PR, not to redesign its logic |

**Key insight:** Every one of this phase's "don't hand-roll" items is a case where a second, DevFlow-authored implementation of a judgment some other system already makes authoritatively (GitHub's rule engine, git's own signature verification, cargo's own duplicate-version rejection, crates.io's own database) would have to be kept in sync with that system forever. That synchronization burden is the single named root cause of two entire backlog items being permanently deleted (999.50, 999.54) rather than fixed — this phase must not reintroduce the pattern under a new name.

## Runtime State Inventory

**Trigger check:** This phase is a rebuild-not-rename, but it operates in the same domain (release mechanics) as multiple prior runtime-state incidents. Applying the inventory as a precaution:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | **None found that this phase creates or must migrate.** No release-progress ledger exists in the current codebase (Phase 26's ledger never merged) — nothing to migrate away from. | None |
| Live service config | **GitHub repo settings, not in git:** `allow_auto_merge: true`, `allow_squash_merge: true`, `allow_merge_commit: true`, `allow_rebase_merge: false`, `delete_branch_on_merge: true` (verified live, `gh api repos/denniyahh/devflow`); two rulesets (`develop-merge-or-squash`, `main-squash-only`) with 0 required approvals, 0 bypass actors, `enforcement: active` (verified live). These are read by 29b's discovery step, never written by this phase — no migration needed, but the planner should treat these as a live external dependency that can change out from under the code without a code change (see Environment Availability). | Code reads these dynamically at runtime; do not cache/hardcode |
| OS-registered state | None applicable — no scheduler/service registration involved in this phase. | None |
| Secrets/env vars | `devflow.releaseSigningKey` (git config, per-machine, already documented in CONTRIBUTING.md) — this phase's tag step reads it but does not create, rename, or migrate it. `user.signingkey` (separate, ordinary-commit key) — untouched. | None — code reads an existing, unrenamed config key |
| Build artifacts | None applicable — this phase adds new Rust modules/functions, no build-artifact renaming. | None |

**Local tag namespace (IN-01) — the one genuine collision, verified live against this repo's own code:** `hooks_after_ship`'s `Hook::VersionBump` (`crates/devflow-core/src/hooks.rs:278-337`) already creates a **local, lightweight, unsigned** `v{version}` tag (`git.rs:223-226`'s `tag()`, `-c tag.gpgSign=false`) on **every ordinary phase Ship** — not just release cuts. This runs today, unconditionally, whenever any phase reaches Ship. The release executor's 29c step must therefore create a **signed, annotated** tag of the same name. Under the derived-state model this is not corruption (Phase 26's framing) — it is a genuine local-vs-remote, unsigned-vs-signed naming collision the 29c tag step must detect and resolve explicitly (see Common Pitfalls, Pitfall 1) rather than a state-machine "namespace violation" needing a ledger to arbitrate.

## Common Pitfalls

### Pitfall 1: The local `hooks_after_ship` tag and the release-cut signed tag share a name

**What goes wrong:** `hooks_after_ship`'s `VersionBump` hook has already created a local, unsigned, lightweight `v{version}` tag on the same checkout by the time the release executor's 29c step tries `git tag -s vX.Y.Z`. `git tag -s <name>` on an existing tag name fails (git refuses to overwrite a tag without `-f`).
**Why it happens:** Two independent code paths (an existing per-Ship hook, and this phase's new release-cut tag step) both compute and write to the same `v{version}` namespace, with different objects (unsigned local vs. signed remote-intended).
**How to avoid:** 29c's tag step must first check: does a local tag `vX.Y.Z` already exist? If yes, is it the same commit and could it simply be re-signed/replaced (`git tag -s -f`, matching the pattern the pre-push hook's own remediation text already documents at `scripts/hooks/pre-push:91` — `git -c user.signingkey=$RELEASE_KEY tag -s -f $tag_name -m '...'`)? Never silently `-f` over a tag that already correctly names the release commit and is signed — check first (observe), only overwrite an unsigned/lightweight duplicate. This dissolves the same way ROADMAP.md's Phase 29 entry predicts: "an ordinary observation ('the tag exists, so do not create it')" — but the exact branch (unsigned local tag exists vs. signed remote tag exists) needs to be designed, not hand-waved.
**Warning signs:** `git tag -s` exits non-zero with "tag 'vX.Y.Z' already exists" on a machine where any prior phase Ship has ever run.

### Pitfall 2: crates.io index lag makes "not yet visible" indistinguishable from "not published" on the wrong endpoint

**What goes wrong:** Querying the sparse index (`index.crates.io`) or running `cargo info`/`cargo search` immediately after `cargo publish` can report "not found" for up to a few seconds (CDN propagation) even though the publish already succeeded at the registry's database. This is the exact mechanism behind the real `cargo publish` message this repo already observed: *"waiting for devflow-core 2.2.0 to be available."*
**Why it happens:** The sparse index is served through a CDN (per `cargo`'s own maintainers, delay is typically <1s but has been observed up to ~3s, and GitHub issue #11314 documents cases with a JFrog-fronted sparse registry timing out entirely); the crates.io `/api/v1` JSON API is backed directly by the database and has no such lag layer.
**How to avoid:** Use the `/api/v1/crates/{name}/{version}` endpoint (verified live this session: <1s response, immediately correct for both a real and fabricated version) as the sole oracle for "is this published," never the sparse index or `cargo search`/`cargo info`. If the pre-publish check must also account for the *just-published* case within the same run (D-04's own idempotency check), a short bounded retry (a handful of seconds, not `cargo publish`'s own multi-minute wait behavior) against the `/api/v1` endpoint is reasonable — but this is a genuinely different oracle than the sparse index `cargo publish` itself waits on, so DevFlow's own wait, if any, should not assume it needs to match `cargo`'s internal wait-for-publish timeout behavior.
**Warning signs:** A publish that visibly succeeded (`cargo publish` exited 0) followed immediately by an observer that reports `NotPublished` — this is index lag on the wrong endpoint, not a real failure.

### Pitfall 3: `gh pr merge --auto` with no method flag silently picks squash

**What goes wrong:** This repo has already suffered this exact failure once, on 2026-07-27, on the sync-back PR: enabling auto-merge without an explicit method defaulted to squash, destroying the ancestry link `scripts/sync-main-to-develop.sh` exists to create, requiring a second repair PR.
**Why it happens:** GitHub's auto-merge UI/API defaults to squash when no method is specified, regardless of what other methods the branch's ruleset allows.
**How to avoid:** Every `gh pr merge --auto` invocation in this phase must pass an explicit method flag (`--squash` or `--merge`), chosen per Pattern 2 above — never bare `--auto`.
**Warning signs:** A PR that shows as "merged" in the executor's observation but the ancestry check (`git merge-base --is-ancestor origin/main origin/develop`) still fails afterward — this is the tell-tale sign of a silently-squashed sync PR.

### Pitfall 4: `git verify-tag`/signature checks require the signer's public key to be resolvable in the checking environment

**What goes wrong:** Confirming a tag is genuinely *signed* (not just annotated) via `git verify-tag` or `git tag -v` requires the checker to have the signer's public key available (in a keyring, or as an SSH allowed-signers file). An unattended CI or fresh clone running 29a's observer may not have this configured, even though the tag itself is genuinely, correctly signed.
**Why it happens:** Signature *verification* is a separate capability from signature *presence* — a tag can be provably signed (has a signature block in its object) without the local machine being able to confirm *whose* signature it is.
**How to avoid:** Treat "cannot verify" (key unavailable) as its own outcome, distinct from both "signed and verified" and "not signed" — this is another instance of the `Unreachable ≠ Absent` principle applied to a local-tooling gap rather than a network gap. `git cat-file tag <name>` inspecting for a `-----BEGIN SSH SIGNATURE-----`/PGP block is a weaker but always-available fallback that answers "has a signature block" without needing key material, and may be the more appropriate 29a check (existence of a signature) vs. reserving true cryptographic verification for 29c's own tag-creation step, which runs in the signer's own environment where the key IS available by construction.
**Warning signs:** `devflow release status` reporting a signed release as "unsigned" simply because it ran somewhere without the maintainer's public key configured.

## Code Examples

### Live-verified: distinguishing a genuinely absent tag from network failure

```bash
# Source: this session's live invocation against the real repo
$ git ls-remote --tags origin
# ... v1.7.0, v1.8.0, v1.8.1, v2.0.0, v2.1.0 (with peeled ^{} entries for each)
# NO v2.2.0 entry at all — reproduces ROADMAP.md's "signed tag never created" claim exactly
```

### Live-verified: crates.io API as the publish-state oracle

```bash
# Source: this session's live invocation, https://crates.io/api/v1/crates/devflow-core
$ curl -s -A "devflow-research (research@devflow)" \
    https://crates.io/api/v1/crates/devflow-core | head -c 200
{"crate":{"id":"devflow-core", ... "max_version":"2.2.0", "newest_version":"2.2.0", ...}}

$ curl -s -o /dev/null -w "%{http_code}\n" \
    -A "devflow-research (research@devflow)" \
    https://crates.io/api/v1/crates/devflow-core/999.999.999
404

$ curl -s -A "devflow-research (research@devflow)" \
    https://crates.io/api/v1/crates/devflow-core/2.2.0 | head -c 200
{"version":{"id":2902316,"crate":"devflow-core","num":"2.2.0", ... "yanked":false, ...}}
```
Note: crates.io's data-access policy expects a descriptive `User-Agent`; a real implementation should include a real contact/repo URL, not the placeholder used for this research probe.

### Live-verified: ruleset discovery, exactly as ROADMAP.md's Phase 29 section records

```bash
# Source: this session's live invocation, gh api repos/denniyahh/devflow/rulesets
$ gh api repos/denniyahh/devflow/rulesets --jq '.[] | {id, name, target, enforcement}'
{"enforcement":"active","id":19616771,"name":"develop-merge-or-squash","target":"branch"}
{"enforcement":"active","id":19616766,"name":"main-squash-only","target":"branch"}

# develop's ruleset, full parameters:
{"required_approving_review_count":0, "allowed_merge_methods":["merge","squash"], ...}
"required_status_checks":[{"context":"Test"},{"context":"Clippy"},{"context":"Format"},
                           {"context":"Build + test in devcontainer"}]
"bypass_actors":[], "current_user_can_bypass":"never"
```
**Note the exact check context name is `"Build + test in devcontainer"`**, not the shorthand "devcontainer build" ROADMAP.md's prose uses — the planner/implementer should always read the live `required_status_checks[].context` strings from `gh api`, never hardcode the prose shorthand, consistent with the "discover, don't copy" design rule.

### Live-verified: repo-level merge settings that make auto-merge viable

```bash
# Source: this session's live invocation, gh api repos/denniyahh/devflow
$ gh api repos/denniyahh/devflow --jq \
    '{allow_auto_merge, allow_squash_merge, allow_merge_commit, allow_rebase_merge, delete_branch_on_merge}'
{"allow_auto_merge":true,"allow_merge_commit":true,"allow_rebase_merge":false,
 "allow_squash_merge":true,"delete_branch_on_merge":true}
```

### Existing, reusable: the git tag command form (verbatim from CONTRIBUTING.md, D-10's literal target)

```bash
# Source: CONTRIBUTING.md § "Cutting a Release" step 5 (this repo's own
# documented, already-proven procedure — 29c must run this exact form,
# not a variant)
git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" \
    tag -s vX.Y.Z <commit> -m "vX.Y.Z"
git push origin vX.Y.Z
# Verify with:
git tag -v vX.Y.Z
```

### Existing, reusable: `publish_order` (do not recompute)

```rust
// Source: crates/devflow-core/src/git.rs:580-606 (already in-tree, tested)
pub fn publish_order(project_root: &Path) -> Vec<String> {
    // topologically sorts workspace-local-path members by their
    // [dependencies] graph, e.g. ["devflow-core", "devflow"]
}
```

## State of the Art

| Old Approach (Phase 26) | Current Approach (Phase 29) | When Changed | Impact |
|--------------------------|------------------------------|---------------|--------|
| Local resume ledger recording step-completion intent | Derived state — every step's completion is computed fresh from remote refs/registry, never recorded | Operator decision, 2026-07-31 | Dissolves CR-05 (ledger permanently bricking the release path) and CR-02 (ledger claiming Complete while nothing published) by construction — "no in-flight state to get stuck in" |
| Direct push to `develop` for version-bump and sync-back commits, requiring an operator-configured ruleset bypass | PR-based merges for every `develop`/`main`-bound change, using GitHub's already-enabled 0-required-approvals + auto-merge | Live ruleset measurement, 2026-07-31 (recorded in ROADMAP.md, re-verified this session) | Retires W-17 entirely — "the executor's direct-push step cannot land ... until the operator adds a bypass" is no longer true because the design no longer needs a bypass |
| `mutating_project_root`'s cwd-vs-`--show-toplevel` comparison, bypassed by an inherited `GIT_DIR` (CR-01) | `git_command`/`hermetic_command` unconditionally scrubbing all 18 repo-local/redirecting git env vars before every invocation | Phase 27, delivered before this phase (999.39) | CR-01's root cause — "no root guard is trustworthy until [999.39] lands" — is closed; this phase's new code must use `git_command`/`hermetic_command` for every git invocation to inherit this fix, and the equivalent scrubbing for `gh`/`cargo`/`curl` invocations should be considered (see Open Questions) |
| Fragile `cargo info` stderr-substring parsing for "is this version published" (WR-04, escalated to a Critical-contributing cause because the ledger supplied a version that could diverge from the manifest) | crates.io `/api/v1/crates/{name}/{version}` JSON endpoint, HTTP-status-code-based | This research session | WR-04 dissolves along with the ledger it depended on — the version being checked comes from a fresh `compute_version()`/manifest read each time, not a stored value that could have drifted |

**Deprecated/outdated:**
- `check_ssh_signing_viability`/`check_signing_viability` as a pre-gate for the release executor: explicitly excluded by D-10, twice (999.50, 999.54 both deleted from the backlog entirely). Do not resurrect as "while we're in the area."
- Any reliance on the sparse crates.io index (`index.crates.io`) as the publish-state oracle: superseded by the direct `/api/v1` endpoint for this use case, per Pitfall 2.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The `curl` shell-out (vs. adding `ureq`) is the right default for the crates.io HTTP call, matching this codebase's established no-new-HTTP-dependency convention | Standard Stack / Alternatives Considered | Low — this is a style/dependency-philosophy judgment, not a correctness claim; either approach is functionally sound, and the alternative is explicitly documented with its own legitimacy-gate requirement if chosen instead |
| A2 | `git cat-file tag <name>` signature-block presence is an adequate 29a-level "has a signature" check, with true cryptographic verification reserved for 29c's own signer-side environment | Common Pitfalls (Pitfall 4) | Medium — if the planner decides 29a itself must do full `git verify-tag` cryptographic verification (not just presence), the design needs a way to make the maintainer's public key available wherever `devflow release status` runs, which is a nontrivial dependency this research did not fully scope |
| A3 | `devflow sync` (999.52) should be built as part of 29b rather than as a separate fourth unit | Phase Units / User Constraints (Claude's Discretion) | Low — explicitly flagged as discretion, not a locked call; the planner may split it out without contradicting anything locked |
| A4 | The version-bump PR to `develop` should use `squash` as its merge method (not explicitly stated anywhere in ROADMAP.md/CONTRIBUTING.md as a hard requirement the way the sync PR's `merge` requirement is) | Architecture Patterns (Pattern 2) | Low — squash is consistent with observed prior release PRs in STATE.md (e.g. "squash-merged to develop"), but this specific PR type's method was inferred from precedent, not found as an explicit written rule the way the sync PR's was |

**If this table is empty:** N/A — see above; none of these are HIGH risk, and none contradict a locked decision.

## Open Questions

1. **Where exactly does the authorization-only persisted record live, and what does it contain?**
   - What we know: ROADMAP.md explicitly names this as the one thing that may be persisted ("a minimal persisted record may survive — for authorization only, never for progress") and explicitly defers pinning its shape to "a discuss-phase task."
   - What's unclear: No `29-CONTEXT.md` exists, so this discuss-phase task never ran. The exact shape (a flag on `State`? a separate file? reuse of the existing `--yes-ship`-style gate-ledger attribution pattern?) is undecided.
   - Recommendation: The planner should treat this as the first design decision to make, likely via a `checkpoint:decision` task early in the phase, rather than inferring a shape from the `--yes-ship` precedent without confirming scope (e.g., does one `--yes-release` cover bump-through-publish, matching Phase 26's D-03, or does the reversible/irreversible unit split (29a/29b/29c) imply a narrower per-unit authorization instead? Nothing in the current ROADMAP.md text re-confirms D-03's "one flag, whole sequence" framing under the new unit-split design — it predates the redesign).

2. **Should 29a's HTTP and `gh`/`cargo` invocations go through the same hermetic-environment-scrubbing discipline as `git_command`/`hermetic_command`?**
   - What we know: Phase 27 built `hermetic_command` specifically because "the redirecting variables are inherited all the way down a process tree" and named `cargo`'s own git-invoking build scripts as "the motivating case." `curl` and `gh` are new production call sites this phase introduces.
   - What's unclear: Whether `GIT_DIR`/`GIT_WORK_TREE` etc. can meaningfully redirect a `curl` HTTP call (almost certainly not — they're git-specific) or a `gh` API call (also almost certainly not, `gh` doesn't consult git-repo-local env vars for API calls, only for repo-context resolution like `gh pr create` inferring the current repo). `cargo publish`, however, explicitly IS the "motivating case" already named in `git.rs`'s doc comment — cargo's build scripts and its own git-based operations (e.g. for git-dependency resolution) could be redirected by an inherited `GIT_DIR`.
   - Recommendation: Use `hermetic_command("cargo", dir)` for every `cargo publish` invocation in 29c (already directly supported by the existing function signature). For `curl`/`gh`, no scrubbing is architecturally necessary, but running them via `Command::new` with an explicit `current_dir` pinned to the resolved project root (the same discipline used everywhere else in this codebase) costs nothing and keeps the pattern consistent.

3. **"Review as primary gate, one automated fix round maximum" — explicitly NOT an operator decision.**
   - What we know: ROADMAP.md is explicit that this is "a proposal awaiting confirmation, not a ruling" and instructs future work not to treat it as locked.
   - What's unclear: Whether this phase's own planning/execution should adopt it as a working practice anyway (given it's evidence-backed: two review rounds on Phase 26 found 12 Criticals across two passes, zero of which any of 763 passing tests caught).
   - Recommendation: Note it prominently for the planner and for `/gsd-discuss-phase` (if run before planning) as a question to put to the operator explicitly, rather than silently building it into the plan's verification-loop structure. Do not encode "one fix round maximum" as a hard rule in PLAN.md's verification section without an explicit operator confirmation — per the roadmap's own warning about an earlier draft that promoted an assistant recommendation into a locked decision and was caught and reversed.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `git` | All units | ✓ | 2.55.0 | — (already the sole sanctioned interface, no fallback needed) |
| `gh` CLI, authenticated | 29b (PR create/merge, ruleset discovery), 29a (optionally, for PR-merged-state observation) | ✓ | 2.96.0, logged in as `denniyahh` | If unauthenticated: existing `preflight_gh_auth_check` pattern (`preflight.rs:639`) already gates Ship-stage runs on `gh auth status` — 29b should adopt the same pattern rather than inventing a new one |
| `cargo` | 29c (publish) | ✓ | 1.97.1 | — no fallback; publish is impossible without it, which is the correct hard-stop |
| `curl` | 29a, 29c (crates.io API queries) | ✓ | 8.21.0 | Nearly universal on Linux/macOS dev machines; if genuinely absent, this is a case for the "missing dependency, no fallback" bucket unless the `ureq` alternative (Alternatives Considered) is adopted instead |
| Network reachability to `github.com`/`api.github.com` | 29a, 29b | ✓ (this session) | — | None — this is precisely the `Unreachable` arm every observe step must handle explicitly, not a install-time fallback |
| Network reachability to `crates.io` | 29a, 29c | ✓ (this session) | — | Same as above — `Unreachable`, never silently treated as `Absent` |
| The maintainer's signing key material (`devflow.releaseSigningKey`) | 29c (tag creation) | Not verifiable from this research session (host-dependent, per-operator) | — | None by design (D-10) — the tag step attempts the real signed tag and reports git's real failure if the key is unavailable, rather than pre-checking |

**Missing dependencies with no fallback:** None identified as currently missing on the verified host — all four external tools and both network endpoints responded successfully during this research session.

**Missing dependencies with fallback:** None currently missing; `curl` availability is the only item with a documented alternative path if it were ever absent.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (workspace-standard `#[test]`/integration tests under `crates/*/tests/`), matching every existing DevFlow test module (`git.rs`'s `#[cfg(test)] mod tests`, `crates/devflow-cli/tests/release_check.rs` as the direct precedent for this exact command family) |
| Config file | none — no external test-framework config; conventions live in `CONTRIBUTING.md` § Testing notes and the `ai-change-acceptance` skill's `rules/change-acceptance.md` |
| Quick run command | `cargo test -p devflow-core <module_name>` / `cargo test -p devflow <name>` (per-package, per-name — **never `cargo test --exact` with a bare name**, which this project's own acceptance rules flag as a proven false-green trap: it matches nothing and still exits 0) |
| Full suite command | `cargo test --workspace` (per CONTRIBUTING.md "Run all tests"), plus `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` — all three required clean per the `ai-change-acceptance` skill's requirement 4 |

### Phase Requirements → Test Map

| Unit | Behavior | Test Type | Automated Command | File Exists? |
|------|----------|-----------|-------------------|-------------|
| 29a | Six-question observer returns correct state for each of Present/Absent/Unreachable per oracle | unit (pure logic, oracle results injected/faked) | `cargo test -p devflow-core release_observe` | ❌ Wave 0 (new module) |
| 29a | Signed-tag-present observation correctly distinguishes lightweight/annotated/signed via fixture-repo tag objects | unit (real git fixture repo, no network) | `cargo test -p devflow-core release_observe -- tag` | ❌ Wave 0 |
| 29a | crates.io HTTP status parsing (200→Published, 404→NotPublished, timeout/DNS-fail→Unreachable) | unit, with a fake/injectable HTTP layer (see below — this is the genuinely-needs-a-fake case) | `cargo test -p devflow-core release_observe -- publish_state` | ❌ Wave 0 |
| 29b | Merge-method policy selects `merge` for sync-back intent even when `squash` is also allowed | unit (pure function, `required_merge_method`, no I/O) | `cargo test -p devflow-core release_publish -- merge_method` (or wherever this lands) | ❌ Wave 0 |
| 29b | Version bump / changelog / commit_path calls reuse existing tested functions unchanged | covered by existing `hooks.rs`/`version.rs` test suites already in-tree | `cargo test -p devflow-core version hooks` | ✓ (existing) |
| 29b | PR-create / PR-merge integration against a real GitHub PR | **manual-only / live-remote, cannot be hermetic** | N/A — see below | N/A |
| 29c | Signed-tag command form matches CONTRIBUTING.md's documented invocation exactly (argument order, `-c user.signingkey=`, `-s`, message form) | unit, string/argv assertion against the constructed `Command`, no execution | `cargo test -p devflow-core release_publish -- tag_command_form` | ❌ Wave 0 |
| 29c | `publish_order()` result is consumed in the exact returned order, never re-sorted or hardcoded | unit, existing `publish_order` tests already assert the ordering; new test asserts the *consumer* iterates in that exact order | `cargo test -p devflow-core release_publish -- publish_order_respected` | ❌ Wave 0 |
| 29c | A pre-existing local unsigned `v{version}` tag (from `hooks_after_ship`) is detected and handled before attempting `git tag -s` | unit, real git fixture repo reproducing the IN-01 collision (create the local lightweight tag exactly as `hooks.rs`'s `version_bump` does, then run 29c's tag step against it) | `cargo test -p devflow-core release_publish -- tag_namespace_collision` | ❌ Wave 0 |
| 29c | Real `cargo publish` / real `git tag -s`/`git push` against a live remote and live registry | **cannot be hermetic — genuinely irreversible against the live world** | Manual UAT only, gated by `checkpoint:human-verify` per the package-legitimacy/irreversible-op conventions already used elsewhere in this project | N/A |

### What can be tested hermetically vs. what genuinely requires a live remote

**Hermetic (fixture-based, no network):**
- Every pure classification function (three-way oracle results, merge-method policy, publish-order consumption order, tag command-form construction).
- Tag-namespace-collision detection: build a real local git fixture repo, create a lightweight unsigned tag exactly as `hooks.rs::version_bump` does (`git.tag()`), then exercise 29c's pre-tag check against it — this reproduces IN-01 without needing any remote.
- Signed-vs-annotated-vs-lightweight tag classification: buildable with a fixture repo and a throwaway test signing key (this project's existing test harness already disables gpgsign/tag.gpgsign for isolation — `git.rs`'s `init_repo()` fixture is the direct precedent), then presence-check via `git cat-file tag`.

**Genuinely requires a live remote (cannot be faked without losing the property under test):**
- Whether `gh pr merge --auto <method>` actually causes GitHub to wait for and then merge on green checks — this is GitHub's own server-side behavior, unfakeable locally. Test via a real (throwaway/disposable, or the project's own feature branches) PR against a real GitHub repo, run as manual UAT, not CI.
- Whether the crates.io `/api/v1` endpoint's HTTP semantics (200/404) hold as documented — verified once, live, in this research session; a live-remote smoke test (not run on every `cargo test --workspace`, but perhaps a `#[ignore]`-gated test run manually) is reasonable to keep this assumption from silently rotting, per this codebase's existing pattern of `#[ignore]`-gated or manually-invoked live checks elsewhere.
- The real `git tag -s` / `git push origin vX.Y.Z` / `cargo publish` sequence's interaction with the live `scripts/hooks/pre-push` fingerprint check and the live crates.io registry — these are 29c's actual irreversible operations and cannot be tested against a fake without testing something other than the real thing. This is precisely why ROADMAP.md's design lesson says "treat adversarial review as the primary gate . . . not the suite" for exactly this class of step.

**How to fake/inject the oracles (for the hermetic cases above):**
- The crates.io HTTP call should be structured so the classification logic (`match status_code { "200" => ..., "404" => ..., }`) is a pure function separable from the `curl` invocation itself — inject a fake status string in unit tests, exercise the real `curl` invocation only in a manual/`#[ignore]`-gated smoke test. This satisfies the `ai-change-acceptance` skill's rejection pattern #2 ("tests that reproduce the production algorithm inside the test body") by testing the classification boundary, not re-implementing the HTTP call.
- The `gh pr merge` method-selection logic should similarly be a pure function (`required_merge_method`) taking `allowed: &[&str]` as a parameter, tested with fixture arrays representing both this repo's real live values (verified this session) and hypothetical other configurations, never requiring a real `gh api` call in the unit test.
- The tag-collision detection should use a **real** local git fixture repo (not a mock) — this project's established convention (`git.rs`'s `init_repo()`/`flow()` helpers) already does this well, and per the `ai-change-acceptance` skill, a fixture-repo-based test IS an assertion at a real boundary (git's own object model), not a reimplementation.

### Sampling Rate

- **Per task commit:** `cargo test -p devflow-core <new_module_name>` / `cargo test -p devflow <new_module_name>` (fast, package-scoped, per the project's existing convention)
- **Per wave merge:** `cargo test --workspace` plus `cargo clippy --workspace --all-targets -- -D warnings` plus `cargo fmt --check`
- **Phase gate:** Full suite green before `/gsd-verify-work`, PLUS a manual UAT pass exercising at minimum one real observe-only run of `devflow release status` against this actual repo's real state (safe — read-only) before any plan claims 29a "done." 29b/29c's actual irreversible operations should not be UAT-exercised against the real repo casually — treat as `checkpoint:human-verify` gated, consistent with this project's existing irreversible-operation review discipline (see `/home/denniyahh/.claude/projects/-var-home-denniyahh-Github-devflow/memory/project-irreversible-ops-need-review-not-tests.md`-class prior finding: Phase 26 had 763 green tests and 11/11 verification while carrying 12 Criticals, none test-caught).

### Wave 0 Gaps

- [ ] `crates/devflow-core/src/release_observe.rs` (or equivalent module name) — new file, no existing test file to extend
- [ ] `crates/devflow-core/src/release_publish.rs` (or equivalent) — new file
- [ ] A shared fixture helper reproducing the `hooks_after_ship`-style local unsigned tag, for the IN-01 collision regression test (can likely reuse/extend `git.rs`'s existing `init_repo()`/`flow()` test helpers rather than duplicating them)
- [ ] Decide test-module home for `gh`/`curl` invocation boundary logic — likely alongside `release_observe.rs`/`release_publish.rs` rather than a new top-level test file, matching this codebase's `#[cfg(test)] mod tests` convention (used throughout `git.rs`, `hooks.rs`, `preflight.rs`)
- [ ] Framework install: none — `cargo test` is already fully configured workspace-wide

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | Partial | `gh` CLI's own token-based auth (`gh auth status`, already checked at preflight); this phase adds no new auth mechanism, reuses the existing pattern |
| V3 Session Management | No | Not applicable — no session concept in this CLI tool |
| V4 Access Control | Yes | Enforcement is delegated entirely to GitHub's branch protection rulesets (0 required approvals is a *repo-owner* access-control decision this phase must respect, not weaken) and to `scripts/hooks/pre-push`'s fingerprint check for the signing key — this phase must not bypass either |
| V5 Input Validation | Yes | Version strings, PR titles/branch names constructed from user/git-derived data must be shell-escaped consistently with the existing `shell_quote` helper (`ship.rs:372-386`) wherever a new shell invocation is built; crates.io API responses (untrusted external input) must be parsed defensively (HTTP status code is the primary signal, never `eval`/deserialize-and-trust JSON body content into a decision without validation) |
| V6 Cryptography | Yes — never hand-rolled | Tag signing is 100% delegated to git's own `-s` signing and the pre-push hook's fingerprint comparison; this phase must add **zero** cryptographic logic of its own (D-10) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Command injection via unsanitized version strings/branch names passed to `git`/`gh`/`cargo`/`curl` `Command` args | Tampering | Use `Command::args([...])` (already the exclusive pattern throughout this codebase — no shell string interpolation anywhere in `git.rs`), never `Command::new("sh").arg("-c").arg(format!(...))` with untrusted content interpolated |
| Inherited `GIT_DIR`/`GIT_WORK_TREE` redirecting a `cargo publish` invocation's git-dependent build-script behavior to an unrelated repository | Tampering/Spoofing | `hermetic_command("cargo", dir)` (already exists, already the documented "motivating case" for cargo specifically) |
| Untrusted/MITM'd crates.io API response influencing an irreversible decision (e.g. a spoofed 404 causing a duplicate-publish attempt) | Tampering | HTTPS is already enforced by the `https://crates.io` URL scheme (curl validates the TLS cert by default); the real backstop is that `cargo publish`'s own duplicate-version rejection is the authoritative final check regardless of what the pre-check API call reported — the API call is an optimization/UX improvement (skip an already-done step), never the sole gate before an irreversible action |
| Leaking the release signing key material into logs/output | Information Disclosure | The tag command already only ever references the key by **fingerprint** in user-facing text (per `git.rs`'s existing `SigningViability` discipline — "never private key material or a full filesystem path") — any new logging in 29c must follow this exact precedent, never log the raw `devflow.releaseSigningKey` value |
| A malicious or compromised dependency added later without the legitimacy gate | Tampering | This research recommends adding zero new dependencies; if a future change adds one (e.g. `ureq`), the Package Legitimacy Gate protocol (already documented in this agent's own instructions) must run before it's accepted |

## Sources

### Primary (HIGH confidence — verified live this session against the real repo/registry/host)

- `gh api repos/denniyahh/devflow/rulesets` — live ruleset data (both `develop` and `main`, required checks, allowed merge methods, 0 bypass actors), re-confirming ROADMAP.md's 2026-07-31 measurement
- `gh api repos/denniyahh/devflow` — live repo settings (`allow_auto_merge: true`, squash/merge-commit availability)
- `curl https://crates.io/api/v1/crates/devflow-core[/2.2.0|/999.999.999]` — live API shape, 200 vs. 404 behavior
- `git ls-remote --tags origin` (this repo) — live confirmation that `v2.2.0` is genuinely absent from the remote while `v2.1.0` is present with a peeled entry
- Direct reads of this repo's own source: `crates/devflow-core/src/git.rs` (full file), `crates/devflow-core/src/hooks.rs` (lines 1-340), `crates/devflow-core/src/ship.rs` (full file), `crates/devflow-cli/src/preflight.rs` (structure + gh-auth-check region), `crates/devflow-cli/src/commands.rs` (release_check region, doctor region), `crates/devflow-cli/src/main.rs` (Command::Release definition + dispatch), `crates/devflow-core/src/version.rs` (compute_version/read_version signatures), `CONTRIBUTING.md` (§ Release signing, § Cutting a Release, full), `scripts/sync-main-to-develop.sh` (full), `scripts/hooks/pre-push` (fingerprint-check region), `.claude/skills/ai-change-acceptance/SKILL.md` + `rules/change-acceptance.md`, `.planning/config.json`
- `.planning/ROADMAP.md` lines 2033-2207 (Phase 29 section, primary context source) and lines 556-620, 1344-1420, 1935-1980 (Phase 26 retro, 999.25 backlog entry, cross-references)
- `.planning/phases/26-release-cut-automation/26-CONTEXT.md` and `26-DISCUSSION-LOG.md` (Phase 26's actual decisions and the discussion that produced/reversed them — read as "what to avoid," per the phase's own instruction)
- `.planning/phases/26-release-cut-automation/999.25-BACKLOG-DOSSIER.md`

### Secondary (MEDIUM confidence — WebSearch results, cross-checked against official-source framing)

- crates.io API general shape and 4xx/5xx status-code behavior (blog.rust-lang.org "crates.io: API status code changes," Feb 2024) — cross-checked live, matches
- `cargo publish` sparse-index propagation delay and wait-for-publish mechanics (rust-lang/cargo GitHub issues #11314, #11356, #11713; HackMD "Cargo Sparse Indexes") — general characterization only, not independently re-measured beyond this repo's own already-recorded "waiting for devflow-core 2.2.0 to be available" observation
- `gh pr merge --auto`/`gh pr checks --watch` behavior (GitHub CLI docs/man pages, cli/cli issue #8514 on `--auto` not itself blocking, issue #7401 on a "no checks yet" race) — general CLI behavior, not project-specific

### Tertiary (LOW confidence — none used as load-bearing for any recommendation in this document)

- None — every WebSearch-sourced claim above was either cross-checked against a live verification in this session or explicitly scoped as general/background characterization rather than a specific claim about this repository.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every tool (`git`, `gh`, `cargo`, `curl`) verified present and working on the live host this session; no new Rust dependencies recommended, so no version-drift risk
- Architecture: HIGH for the discovered facts (rulesets, repo settings, API shapes, existing code to reuse), MEDIUM for exact module/function shape (explicitly Claude's discretion, no CONTEXT.md to further constrain it)
- Pitfalls: HIGH — three of four pitfalls are either reproductions of documented live incidents in this exact repository (the sync-PR squash incident, the v2.2.0 signed-tag-never-created incident) or direct consequences of already-existing, already-read code (`hooks_after_ship`'s tag collision)

**Research date:** 2026-07-31
**Valid until:** ~14 days for the GitHub ruleset/repo-settings facts (operator-configurable, could change without a code change — the design rule itself accounts for this by re-discovering at runtime rather than trusting this document's snapshot); ~30 days for the crates.io API shape and cargo/git/gh CLI behavior (stable, slow-moving public APIs)
