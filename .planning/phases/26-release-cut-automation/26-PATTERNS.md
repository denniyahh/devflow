# Phase 26: Release-Cut Automation - Pattern Map

**Mapped:** 2026-07-29
**Files analyzed:** 9 (new/modified)
**Analogs found:** 9 / 9 (RESEARCH.md already did deep source-reading; this
document distills it into copy-from excerpts per file)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|--------------------|------|-----------|-----------------|---------------|
| `crates/devflow-core/src/sync.rs` (NEW) | service | file-I/O + event-driven (git mutation) | `scripts/sync-main-to-develop.sh` (script→Rust port) + `git.rs`'s `GitFlow` methods | exact (logic), role-match (language) |
| `crates/devflow-core/src/git.rs` (+push_develop/tag_signed/publish helpers) | service | event-driven (subprocess mutation) | same file's existing `push`/`tag`/`release_finish`/`publish_order` | exact |
| `crates/devflow-core/src/version.rs` (+changelog-grouping fn) | utility/transform | transform | same file's `classify_range_bump`/`classify_commit_message` | exact (sibling function, not a modification) |
| `crates/devflow-core/src/ship.rs` (`prepend_changelog` signature change) | utility/transform | transform | same file's existing `prepend_changelog` | exact |
| `crates/devflow-core/src/hooks.rs` | — | — | NOT modified this phase (different lifecycle event — Ship vs Release); read as precedent only |
| `crates/devflow-cli/src/main.rs` (`Command::Release` grows fields / new `Command::Sync`) | route/CLI-surface | request-response | same file's `Command::Start { yes_ship, .. }` and existing `Command::Release { check, project }` | exact |
| `crates/devflow-cli/src/commands.rs` (`release_execute`, `sync_cmd`) | controller | request-response, orchestrates event-driven steps | same file's `release_check` + its `Check`-producing helpers (`check_publish_order`, `check_signing`, `check_divergence`) | exact |
| `crates/devflow-cli/tests/release_execute.rs` (NEW) | test | — | `crates/devflow-cli/tests/release_check.rs` (existing, same shape) | exact |
| `crates/devflow-core/src/git.rs` test module (`init_repo`/`flow`/bare-remote fixture) | test | — | same file's existing `init_repo()`/`flow()` (`git.rs:969-987`) | exact — extend, don't replace |

## Pattern Assignments

### `crates/devflow-core/src/sync.rs` (NEW — service, file-I/O/event-driven)

**Analog:** `scripts/sync-main-to-develop.sh` (full script, 66 lines) ported
1:1 into Rust, using `git.rs`'s `Command::new("git").args([...])` idiom and
`GitFlow`-style error type.

**Imports pattern** (model on `git.rs:1-7`):
```rust
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};
```

**Core pattern — the exact checks that MUST survive the port**, from
`scripts/sync-main-to-develop.sh`:
```bash
# 1. Clean tree
if [ -n "$(git status --porcelain)" ]; then exit 1; fi
# 2. On develop
CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$CURRENT_BRANCH" != "develop" ]; then exit 1; fi
# 3. Fetch first
git fetch origin main develop --quiet
# 4. Short-circuit (D-06's per-step idempotency shape, generalized elsewhere)
if git merge-base --is-ancestor origin/main HEAD; then
    echo "origin/main is already an ancestor of develop — nothing to sync."
    exit 0
fi
# 5-8. Tree-identity safety property (D-09, the load-bearing check — never relax)
BEFORE_TREE="$(git rev-parse HEAD^{tree})"
git merge -X ours origin/main --no-edit -m "<message>"
AFTER_TREE="$(git rev-parse HEAD^{tree})"
if [ "$BEFORE_TREE" != "$AFTER_TREE" ]; then
    # refuse — leave develop untouched, do not push
    exit 1
fi
# 9. D-08's addition — the script's own final manual instruction, now automated:
git push origin develop
```

**Error handling pattern** — mirror `git.rs`'s `GitError` enum shape
(`git.rs:10-18`):
```rust
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("failed to execute git: {0}")]
    Io(#[from] std::io::Error),
    #[error("git command failed: {0}")]
    Command(String),
}
```
Add a `SyncError` (or extend `GitError`) with a distinct variant for the
tree-mismatch refusal per D-09/Pattern 4's "fail-closed, don't push" shape —
this refusal must be a typed, distinguishable error, not a generic
`Command(String)`, since D-06's resume logic and the CLI's exit messaging
both need to tell "refused, tree changed" apart from "git command failed."

**Testing pattern** — extend, don't replace, the existing fixture
(`git.rs:969-987`, exact text):
```rust
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
```
For push-testing add a second `init_repo()`-style bare dir as
`git remote add origin <bare-path>` — this is the project's stated way to
test `git push` hermetically (RESEARCH.md's Code Examples section, already
verified as the right shape).

---

### `crates/devflow-core/src/git.rs` (+ push_develop / tag_signed / cargo_info_check / cargo_publish)

**Analog (push):** existing `push` (`git.rs:222-225`):
```rust
/// Push `branch` to `origin`, setting upstream.
pub fn push(&self, branch: &str) -> Result<(), GitError> {
    info!("pushing branch: {branch}");
    self.git(["push", "-u", "origin", branch])
}
```
New develop-push code should sit alongside this, NOT reuse
`release_start`/`release_finish` (`git.rs:114-138`, dead in production,
unsigned lightweight tags, no PR gate — explicit anti-pattern per
RESEARCH.md) and NOT reuse `hooks_after_ship`'s `version_bump`
(`hooks.rs:268-296`, local-only, never pushes).

**Analog (tag, doc-comment discipline to copy):** existing `tag`
(`git.rs:140-151`) shows the project's convention of documenting *why* a
flag is scoped (`-c tag.gpgSign=false` explained in terms of a Phase 13
dogfood incident). The new signed-tag function must carry equivalent doc
rationale for D-10's exact invocation form:
```rust
// Verbatim form required by D-10 (CONTRIBUTING.md § "Cutting a Release" step 5):
// git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" \
//     tag -s vX.Y.Z <commit> -m "vX.Y.Z"
```
Never build a viability predictor (`check_ssh_signing_viability`,
`git.rs:811`, explicitly must not gain a new caller from this phase).

**Analog (publish order reuse):** `publish_order` (`git.rs:516-542`) —
call this unchanged; do not recompute the topo-sort.

**Analog (cargo_info_check — new pattern, no existing analog in this repo,
but same "shell to the real tool" idiom as every other `git.rs` function):**
```rust
fn crate_already_published(name: &str, version: &str) -> Result<bool, PublishCheckError> {
    let output = Command::new("cargo")
        .args(["info", &format!("{name}@{version}"), "--registry", "crates-io"])
        .output()?;
    if output.status.success() {
        return Ok(true); // D-06: already live, skip publish
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("could not find") {
        return Ok(false);
    }
    Err(PublishCheckError::Ambiguous(stderr.trim().to_string()))
}
```
(Source: RESEARCH.md Pattern 4, itself derived from live-verified
`cargo info` output against this checkout — not a hypothetical.)

**Testing pattern:** existing `publish_order_derives_core_before_cli_from_a_fixture_workspace`
(`git.rs:1522`) — same fixture-workspace-in-tempdir shape for any new
publish-check unit test.

---

### `crates/devflow-core/src/version.rs` (+ changelog-grouping function, sibling to `classify_range_bump`)

**Analog:** `classify_range_bump` + `classify_commit_message`
(`version.rs:393-453`).

**Critical distinction (Pitfall 1 — do not miss this):** `classify_range_bump`
folds every commit down to a single `Bump` enum value via `bump.max(this_bump)`
(`version.rs:410-421`) — it is NOT reusable as changelog content. The new
function must walk the same range with the same `git log --no-merges <range>
--format=%H%x1f%B%x1e` idiom and the same `git_conventional::Commit::parse`
call, but *collect* subjects into groups instead of folding:
```rust
// Reuse verbatim: the %x1f/%x1e record-separator idiom (version.rs:399-421)
let output = Command::new("git")
    .args(["log", "--no-merges", &range, "--format=%H%x1f%B%x1e"])
    .current_dir(project_root)
    .output()?;
// ... then per record, git_conventional::Commit::parse(message), and instead
// of bump.max(this_bump), push commit.description() into a
// BTreeMap<Type, Vec<String>> keyed by feat→Added, fix|perf→Fixed,
// breaking→Breaking (RESEARCH.md Open Question 2's recommended default —
// omit docs/chore/test/ci/refactor/style entirely).
```

**Genuinely reusable as-is (no change needed):** `release_range_start`
(`version.rs:301-357`, not re-read here — already cited by RESEARCH.md) and
`reachable_semver_baseline`/`highest_semver_tag` (`version.rs:183-260`) for
the baseline/range anchor — call these, do not reimplement.

---

### `crates/devflow-core/src/ship.rs` (`prepend_changelog` — content-source swap)

**Analog:** existing `prepend_changelog` (`ship.rs:391-407`), exact text:
```rust
pub fn prepend_changelog(existing: &str, version: &str, date: &str) -> String {
    const HEADER: &str = "# Changelog\n\n\
        All notable changes to this project are documented here.\n";
    let entry = format!("## {version} — {date}\n\n- Released phase via DevFlow.\n");

    if existing.trim().is_empty() {
        return format!("{HEADER}\n{entry}");
    }
    if let Some(idx) = existing.find("\n\n") {
        let (head, tail) = existing.split_at(idx + 2);
        format!("{head}{entry}\n{tail}")
    } else {
        format!("{entry}\n{existing}")
    }
}
```
D-12's change: replace the hardcoded `"- Released phase via DevFlow.\n"`
literal with content assembled from the new grouped-changelog function above
(e.g. `### Added\n- ...\n### Fixed\n- ...`) — the header-insertion / no-header
fallback logic (everything else in this function) is unchanged; this is a
pure-transform signature growth, not a rewrite. Existing tests
`prepend_changelog_creates_header_when_empty` / `_inserts_after_header`
(`ship.rs:588-602`, not fully re-read but named here) are the shape new
tests should follow.

**Caller to update:** `hooks.rs`'s `changelog_append` (`hooks.rs:234-266`) —
NOT modified by this phase's Release path directly (it's Ship-time
machinery, a different lifecycle event per RESEARCH.md), but its call site
`crate::ship::prepend_changelog(&existing, &version, &today())` will need
its argument list to match whatever `prepend_changelog`'s new signature
requires, if the signature changes rather than an overload/new function
being added. Confirm during planning whether `prepend_changelog` gains a new
parameter (breaking this call site) or a new sibling function is added
instead (non-breaking) — RESEARCH.md's structure section implies "signature
likely grows," so this call site is an in-scope edit, not incidental.

---

### `crates/devflow-cli/src/main.rs` (CLI surface: `Release` flags / `Sync` command)

**Analog (typed dangerous-flag precedent):** `Command::Start`'s `yes_ship`
field and doc comment (`main.rs:78-90`), exact text:
```rust
/// Pre-authorize the Ship gate so this run can reach a completed
/// Ship stage unattended (D-04/D-05/D-06, 23-09). The Ship gate
/// still fires and is still answered through the normal gate
/// protocol — this only supplies the approval automatically,
/// attributed to `--yes-ship` in the gate ledger. Must be typed on
/// every invocation: it cannot be set in `devflow.toml` or any
/// environment variable (D-05), so an unattended auto-merge can
/// never become a standing, silent default.
#[arg(long)]
yes_ship: bool,
```
`--yes-release` (D-03) must carry an equivalent doc comment and the same
"never a config/env default" constraint — this is the exact precedent to
copy verbatim in spirit, adjusted for the release-cut's own gate semantics.

**Analog (existing `Release` variant to extend):** `main.rs:227-242`:
```rust
/// Read-only release-cut preflight: self-pin, develop/main divergence,
/// crates.io publish order, and tag-signing viability.
///
/// Ceiling is `--check` only (20d) — this command never runs the actual
/// merge/tag/sync/publish sequence, which is a deferred, not-yet-built
/// executor (DEN-50).
Release {
    #[arg(long)]
    check: bool,
    #[arg(default_value = ".")]
    project: PathBuf,
},
```
This doc comment's second paragraph is now WRONG after this phase and must
be rewritten (the executor stops being "deferred"); the `check`/`project`
field-declaration style is the pattern to extend with `execute`/`yes_release`.

**Analog (dispatch arm to extend):** `main.rs:572-586`, the existing
`Command::Release { check, project }` match arm's "reject silently-valid
bare invocation" pattern — the new `execute`/`yes_release` arm should follow
the same "explicit rejection with an explanatory message" shape rather than
silently defaulting.

---

### `crates/devflow-cli/src/commands.rs` (`release_execute`, `sync_cmd`)

**Analog:** `release_check` (`commands.rs:2195-2231`) and its `Check`-struct
producing helpers `check_publish_order` (`commands.rs:2332-2349`),
`check_signing` (`commands.rs:2355-2380`), `check_divergence`
(`commands.rs:2301-2325`) — the "a Vec of typed step results, rendered
uniformly with an icon + fail-aggregation loop" pattern:
```rust
pub(crate) fn release_check(project_root: &Path) -> Result<(), CliError> {
    let checks: Vec<Check> = vec![
        check_self_pin(project_root),
        check_divergence(project_root),
        check_publish_order(project_root),
        check_signing(project_root),
    ];
    let mut failed = false;
    for c in &checks {
        let icon = match c.status.as_str() {
            "ok" => "✓", "warn" => "⚠", "fail" => "✗", _ => "?",
        };
        // ... print, track failed
    }
    if failed { Err(CliError::Message(...)) } else { Ok(()) }
}
```
`release_execute` should reuse `release_check`'s four checks as a hard
pre-gate (per RESEARCH.md's architecture diagram) and then run the five
steps (version bump+push, [human gate], signed tag, sync, publish) as a
similarly-structured sequence — each step consulting a live-state predicate
before acting (D-06), matching this same "typed step, read state, act,
report" shape rather than introducing a new orchestration abstraction.

**`gh` invocation precedent (for context only — D-02 means no new `gh`
call site is added):** `preflight.rs:611-622`:
```rust
/// pass/fail plus a short reason string — raw `gh auth status` stdout/stderr
...
Ok(_) => Err("gh auth status reports not authenticated".to_string()),
```
Confirms the only two existing `gh` call sites in the codebase; this phase
adds none.

---

### `crates/devflow-cli/tests/release_execute.rs` (NEW)

**Analog:** `crates/devflow-cli/tests/release_check.rs` (562 lines, existing
— same integration-test shape: spin up a fixture project root, invoke the
CLI command function, assert on `Check` results / exit behavior). Follow its
existing file structure for the new `release_execute` test file, per
RESEARCH.md's own Wave-0-gap note.

---

## Shared Patterns

### Fail-fast, no-rollback
**Source:** `hooks.rs:156-165` (doc comment on `merge_feature`'s no-rollback
policy) and `hooks.rs:105-112` (`hooks_after_ship`'s terminal batch).
**Apply to:** every release-cut step in `commands.rs::release_execute` and
`sync.rs` — a failed step returns `Err` and stops; nothing already landed
(commit, tag, publish) is ever automatically undone. Copy the "state here
because it must not be re-derived later" doc-comment discipline for the
executor's own top-level function.

### Idempotent step via live-state check (D-06)
**Source:** `scripts/sync-main-to-develop.sh:41-44`'s
`git merge-base --is-ancestor origin/main HEAD` short-circuit — the one
proven precedent for this shape in the codebase.
**Apply to:** every one of the five release-cut steps (push-ahead check,
tag existence+annotated+verified+reachable check, `cargo info` check). No
persisted progress file — resume comes from reading live git/registry
state each run.

### Shell-to-subprocess via argv array, never `sh -c` string interpolation
**Source:** every function in `git.rs` (`Command::new("git").args([...])`),
contrasted with `hooks.rs:213-217`'s one exception (`docs_update`'s
`Command::new("sh").arg("-c")`, which takes no untrusted input — not a
precedent to follow here).
**Apply to:** all new `git`/`cargo` subprocess calls (push, tag, sync merge,
`cargo info`, `cargo publish`) — argv arrays only, never string-interpolated
shell commands, since commit-derived content (branch names, tag names) must
never pass through a shell.

### Error type shape
**Source:** `git.rs:10-18`'s `GitError` enum (`thiserror`, an `Io` variant
via `#[from]`, a `Command(String)` catch-all).
**Apply to:** any new error enum (`SyncError`, `PublishCheckError`) — same
two-variant-plus-specific-variants shape, `thiserror::Error` derive.

### Redaction/truncation discipline for commit-derived text
**Source:** `preflight.rs`'s `truncate_reason` precedent (cited by
RESEARCH.md's Security Domain section; not independently re-read here since
RESEARCH.md already located and described it precisely enough to apply).
**Apply to:** changelog content (commit subjects) and any gate-context error
strings this phase surfaces — same truncation/redaction treatment before
logging or displaying attacker/contributor-influenced text.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `cargo_info_check` / `cargo_publish` functions in `git.rs` | service | event-driven | No production code has ever shelled out to `cargo publish`/`cargo info` before this phase — closest analog is the *idiom* (`Command::new(...).output()`), not a prior function to copy structurally beyond that idiom, per RESEARCH.md's Code Examples section (already the best available reference) |
| Bare-remote push-test fixture (extending `init_repo`/`flow`) | test | — | No existing test in `git.rs` exercises an actual `git push` (zero production push call sites before this phase) — the fixture pattern is a natural extension of `init_repo()` but has no direct prior instance to copy verbatim; RESEARCH.md's Code Examples section gives the recommended shape (`git remote add origin <bare-path>`) |

## Metadata

**Analog search scope:** `crates/devflow-core/src/{git,version,hooks,ship}.rs`,
`crates/devflow-cli/src/{main,commands,preflight}.rs`,
`scripts/sync-main-to-develop.sh`, `crates/devflow-cli/tests/release_check.rs`
**Files scanned:** 9 (all explicitly cited by RESEARCH.md's Sources section;
no additional Glob/Grep sweep was needed since RESEARCH.md's own research
pass already read every relevant file in full and cited exact line ranges)
**Pattern extraction date:** 2026-07-29
