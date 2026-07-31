# Phase 29: Release-Cut Executor — Pattern Map

**Mapped:** 2026-07-31
**Files analyzed:** 8 (new/modified, across 29a/29b/29c)
**Analogs found:** 8 / 8

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/devflow-core/src/release_observe.rs` (NEW, 29a) | service (oracle/observer) | request-response (remote read, three-valued result) | `crates/devflow-core/src/git.rs` — `AncestorStatus`/`origin_main_ancestor_status`, `SigningViability`/`check_signing_viability` | exact |
| `crates/devflow-core/src/release_publish.rs` (NEW, 29c) | service (external-process invocation) | request-response / file-I/O (subprocess) | `crates/devflow-core/src/git.rs` — `git_raw`/`hermetic_command`, `publish_order` | role-match |
| `crates/devflow-core/src/git.rs` (EXTEND: ls-remote wrapper, signed-tag presence, gh-ruleset discovery) | service (git substrate) | request-response | same file, existing `origin_main_ancestor_status`, `check_signing_viability`, `git_command`/`hermetic_command` | exact (in-file extension) |
| `crates/devflow-cli/src/commands.rs` (EXTEND: `release_status`, `release_bump_and_pr`, `release_commit`) | controller (CLI command handler) | CRUD-ish / request-response | same file, existing `release_check` (lines ~2206-2242) + `doctor` (~2008+) + `Check` struct (~2000) | exact |
| `crates/devflow-cli/src/main.rs` (EXTEND: `Command::Release{...}` variants or new subcommands, dispatch arm) | route/CLI arg parsing | request-response | same file, existing `Command::Release { check, project }` (~230-243) + dispatch arm (~575-589) | exact |
| `crates/devflow-cli/src/preflight.rs` (possible EXTEND: reuse `gh auth` pattern for 29b) | middleware (precondition/guard) | request-response | same file, `preflight_gh_auth_check` (~637-657) | exact |
| `crates/devflow-cli/tests/release_status.rs` (NEW, 29a) | test | request-response (drives real binary) | `crates/devflow-cli/tests/release_check.rs` (entire file, 562 lines) | exact |
| `crates/devflow-core/src/git.rs` `#[cfg(test)] mod tests` (EXTEND: fixtures for new git.rs functions) | test | CRUD (git fixture) | same file, `init_repo()`/`flow()`/`git()`/`commit_file()` helpers (~1005-1050) | exact |

## Pattern Assignments

### `crates/devflow-core/src/release_observe.rs` (NEW — service/oracle, 29a)

**Analog:** `crates/devflow-core/src/git.rs` — `AncestorStatus` (lines 533-545), `origin_main_ancestor_status` (553-570ish), `SigningViability` (758-767), `check_signing_viability` (983-988)

This codebase **already has the exact three-valued oracle pattern** the roadmap/research calls for (`Present`/`Absent`/`Unreachable`) — it is not a new idiom to invent, it is `AncestorStatus`/`SigningViability` generalized. Copy this shape directly for all six 29a observations.

**Three-valued enum pattern** (`git.rs:533-545`):
```rust
pub enum AncestorStatus {
    /// `origin/main` is an ancestor of `HEAD` — sync would be a no-op.
    Ancestor,
    /// `origin/main` resolves locally but is NOT an ancestor of `HEAD`.
    Diverged,
    /// `origin/main` does not resolve locally at all (never fetched, or no
    /// remote configured). Distinct from `Diverged` so the caller can
    /// degrade to an actionable message instead of a false divergence.
    RefAbsent,
}
```
Map directly: `Present` ~ `Ancestor`/`Viable`, `Absent` ~ `Diverged`/`RefAbsent` split as appropriate per-question, `Unreachable { reason: String }` ~ the `SigningViability::Unknown { reason }` arm below — carry a `reason` string, never a bare unit variant, so the CLI can print *why*.

**Oracle function pattern, read-only, never `git fetch`** (`git.rs:553-570`):
```rust
pub fn origin_main_ancestor_status(project_root: &Path) -> AncestorStatus {
    let ref_exists = git_command(project_root)
        .args(["rev-parse", "--verify", "--quiet", "origin/main"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if !ref_exists {
        return AncestorStatus::RefAbsent;
    }
    let is_ancestor = git_command(project_root)
        .args(["merge-base", "--is-ancestor", "origin/main", "HEAD"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false);
    if is_ancestor { AncestorStatus::Ancestor } else { /* ... */ }
}
```
Note: 29a's ref-presence checks (release PR merged, sync merged, tag present) must issue their own `git fetch`/`ls-remote` (unlike `release --check`'s no-fetch ceiling) since they observe *remote* truth, not already-fetched local refs — do not copy the no-fetch constraint, only the read-only-ness and the `git_command` substrate.

**`Unreachable ≠ Absent` fail-soft pattern with a reason string** (`git.rs:758-767`, `983-988`):
```rust
pub enum SigningViability {
    Viable { fingerprint: Option<String> },
    NotViable { reason: String },
    /// Could not be determined — tool absent, format unset with no key, etc.
    /// Fail-soft: never a crash.
    Unknown { reason: String },
}

pub fn check_signing_viability(project_root: &Path) -> SigningViability {
    match git_config(project_root, "gpg.format").as_deref() {
        Some("ssh") => check_ssh_signing_viability(project_root),
        _ => check_gpg_signing_viability(project_root),
    }
}
```
This is the direct precedent for the crates.io `PublishState::{Published, NotPublished, Unreachable { reason }}` enum sketched in RESEARCH.md's Pattern 1 — use the same three-arm shape, same `reason: String` field name convention, same "match on tool/output, degrade instead of panic" structure. Do not invent a new enum shape.

**Hermetic substrate — every git call MUST go through this** (`git.rs:72-94`):
```rust
pub fn git_command(repo: &Path) -> Command {
    hermetic_command("git", repo)
}
pub fn hermetic_command(program: &str, dir: &Path) -> Command {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir);
    for var in REPO_LOCAL_GIT_VARS.iter().chain(ALSO_REDIRECTING_GIT_VARS) {
        cmd.env_remove(var);
    }
    cmd
}
```
For `curl`/`gh` calls (no existing hermetic wrapper needed per RESEARCH.md Open Question 2 — git-redirecting env vars don't affect them), use `Command::new` directly but still pin `.current_dir(project_root)` for consistency, matching the existing project convention (every command in this codebase pins an explicit cwd rather than relying on ambient cwd).

---

### `crates/devflow-core/src/release_publish.rs` (NEW — service, 29c)

**Analog:** `crates/devflow-core/src/git.rs` — `git_raw` (private helper, ~lines 450-470) and `publish_order` (580-606)

**Subprocess invocation + error mapping pattern** (`git.rs`, `git_raw`):
```rust
fn git_raw(&self, args: &[&str]) -> Result<(), GitError> {
    debug!("git {}", args.join(" "));
    let output = git_command(&self.root)
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(GitError::Command(stderr_or_status(&output)))
    }
}

fn stderr_or_status(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() { format!("exited with {}", output.status) } else { stderr }
}
```
Copy this exact shape for `cargo publish` invocation via `hermetic_command("cargo", project_root)` (RESEARCH.md names `cargo` explicitly as the "motivating case" for `hermetic_command`, not just `git_command`) — same `stderr_or_status`-style error extraction, same "exit code decides, never predicted" discipline (D-10).

**`publish_order` — reuse verbatim, never recompute** (`git.rs:580-606`):
```rust
pub fn publish_order(project_root: &Path) -> Vec<String> {
    // topologically sorts workspace-local-path members by their
    // [dependencies] graph, e.g. ["devflow-core", "devflow"]
    ...
}
```
Already used by `release_check`'s `check_publish_order` (`commands.rs:2343-2360`) — the 29c publish step must call the exact same function, not reimplement ordering.

**The literal, undecorated command form for signing (D-10, do not vary)** — from CONTRIBUTING.md, already referenced by `GitFlow::tag`'s doc comment (`git.rs:215-226`) as the pattern this codebase already treats specially (scoped `-c tag.gpgSign=false` override to avoid `$EDITOR` blocking):
```bash
git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" \
    tag -s vX.Y.Z <commit> -m "vX.Y.Z"
git push origin vX.Y.Z
git tag -v vX.Y.Z   # verify
```

---

### `crates/devflow-core/src/git.rs` (EXTEND — ls-remote/tag-presence/ruleset-discovery wrappers)

**Analog:** existing `origin_main_ancestor_status` and `GitFlow::tag` in the same file (see above). New functions should live alongside these, follow the same `pub fn ... (project_root: &Path) -> SomeStatusEnum` free-function shape (not methods on `GitFlow`, matching how `origin_main_ancestor_status`/`check_signing_viability`/`publish_order` are already free functions rather than `GitFlow` methods) for anything that doesn't need `GitFlowConfig`.

**IN-01 collision — the existing local-tag creator this phase's tag step must detect and not blindly overwrite** (`hooks.rs:296-304`, `version_bump` function, and `git.rs:223-226`, `GitFlow::tag`):
```rust
// hooks.rs — runs on EVERY ordinary phase Ship, unconditionally:
let tag = format!("v{version}");
git.tag(&tag)?;
```
```rust
// git.rs:223-226 — the tag this call creates: local, lightweight, UNSIGNED
pub fn tag(&self, tag: &str) -> Result<(), GitError> {
    info!("tagging {tag}");
    self.git(["-c", "tag.gpgSign=false", "tag", tag])
}
```
29c's tag step must check for this pre-existing local unsigned tag before running `git tag -s`, per RESEARCH.md Pitfall 1 — `git tag -s` on an existing name fails without `-f`, and blindly forcing over a *correctly signed* tag would be wrong. Observe first (does a local tag of this name exist, is it already signed at the right commit), matching the `-c tag.gpgSign=false`-scoping precedent's own "check the actual state before acting" discipline.

---

### `crates/devflow-cli/src/commands.rs` (EXTEND — `release_status`, `release_bump_and_pr`, `release_commit`)

**Analog:** `release_check` (lines 2206-2242) + `Check` struct (2000-2005) + `doctor`'s `cmd_check` helper (2011+)

**Command handler shape — list of checks/observations, then report, fail-if-any-fail** (`commands.rs:2206-2242`):
```rust
pub(crate) struct Check {
    pub(crate) name: String,
    pub(crate) status: String,       // "ok" | "warn" | "fail"
    pub(crate) version: Option<String>,
    pub(crate) install_hint: Option<String>,
}

pub(crate) fn release_check(project_root: &Path) -> Result<(), CliError> {
    let checks: Vec<Check> = vec![
        check_self_pin(project_root),
        check_divergence(project_root),
        check_publish_order(project_root),
        check_signing(project_root),
    ];
    let mut failed = false;
    for c in &checks {
        let icon = match c.status.as_str() { "ok" => "✓", "warn" => "⚠", "fail" => "✗", _ => "?" };
        let detail = c.version.as_deref().unwrap_or("-");
        println!("  {:<32} {icon}  {detail}", c.name);
        if matches!(c.status.as_str(), "warn" | "fail") && let Some(hint) = &c.install_hint {
            println!("      — {hint}");
        }
        if c.status == "fail" { failed = true; }
    }
    if failed {
        Err(CliError::Message("release preflight failed — see checks above".into()))
    } else {
        println!("\nrelease preflight passed");
        Ok(())
    }
}
```
`release_status` (29a) should follow this exact shape, one `Check`-producing function per observation, sourced from `release_observe.rs`'s new oracle functions instead of `check_self_pin`/`check_divergence`/etc. The existing `"ok"|"warn"|"fail"` string-status convention should map three-valued Present/Absent/Unreachable to it consistently (e.g. `Unreachable` → `"warn"`, never silently `"ok"`).

**Individual check function pattern — match on the core enum, produce a `Check`** (`commands.rs:2312-2336`, `check_divergence`):
```rust
fn check_divergence(project_root: &Path) -> Check {
    const NAME: &str = "develop/main divergence (origin/main ancestor)";
    match devflow_core::git::origin_main_ancestor_status(project_root) {
        devflow_core::git::AncestorStatus::Ancestor => Check { name: NAME.into(), status: "ok".into(), version: Some("...".into()), install_hint: None },
        devflow_core::git::AncestorStatus::Diverged => Check { name: NAME.into(), status: "fail".into(), version: Some("...".into()), install_hint: Some("...".into()) },
        devflow_core::git::AncestorStatus::RefAbsent => Check { name: NAME.into(), status: "warn".into(), version: Some("...".into()), install_hint: Some("...".into()) },
    }
}
```

**Error type** — `CliError::Message(String)` (defined `main.rs:427-428`) is the catch-all for command-level failures; git/version/hook-specific errors get their own `#[from]` variants. New release-cut error surfaces (gh failures, cargo publish failures) should either add a new `#[error(transparent)]` variant if a structured error type exists, or use `CliError::Message` for ad-hoc cases, matching existing convention — do not invent a parallel error type.

---

### `crates/devflow-cli/src/main.rs` (EXTEND — `Command::Release{...}` / dispatch)

**Analog:** existing `Command::Release { check, project }` variant (lines 230-243) and its dispatch arm (575-589)

**Clap subcommand-variant pattern**:
```rust
Release {
    /// Run the read-only preflight checks. Required: a bare `devflow
    /// release` (omitted `--check`) is rejected rather than silently
    /// treated as a valid run.
    #[arg(long)]
    check: bool,
    /// Project root.
    #[arg(default_value = ".")]
    project: PathBuf,
},
```
**Dispatch arm pattern**:
```rust
Command::Release { check, project } => {
    if !check {
        return Err(CliError::Message("...".to_string()));
    }
    release_check(&project_root(project)?)
}
```
For 29a/29b/29c, either extend this one `Release` variant with new flags/subcommand-like fields, or (Claude's discretion, per RESEARCH.md) add `Command::ReleaseStatus { version, project }`, `Command::ReleaseBumpAndPr { .. }`, `Command::ReleaseCommit { .. }` as sibling top-level variants — either way, copy the `#[arg(default_value = ".")] project: PathBuf` convention and the `project_root(project)?` resolution call used by every other command in this file.

---

### `crates/devflow-cli/src/preflight.rs` (reference for `gh` invocation and fail-soft precondition pattern)

**Analog:** `preflight_gh_auth_check` (lines 637-657) — the only existing `gh`-shelling call site besides `doctor`'s `gh --version` check

```rust
fn preflight_gh_auth_check(state: &State) -> Result<(), String> {
    if !gh_auth_check_applies(state.stage) {
        return Ok(());
    }
    match std::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => Err("gh auth status reports not authenticated".to_string()),
        Err(_) => {
            println!(
                "warning: `gh` binary not found — cannot verify GitHub credential validity \
                 before Ship (fail-soft, not a preflight failure)"
            );
            Ok(())
        }
    }
}
```
Two things to copy: (1) `gh auth status` is checked before any other `gh` call in this codebase — 29b's `gh pr create`/`gh pr merge`/`gh api rulesets` calls should run in an environment where this same check has already gated, or perform an equivalent check first; (2) the raw stdout/stderr of `gh` commands is **never captured or logged** (see the doc comment above this function, T-17-13 — Information Disclosure) — 29b/29c's `gh` output handling must follow the same never-log-raw-credentials-adjacent-output discipline.

**`gh --version` doctor check** (`commands.rs:2112-2117`) — precedent for "is `gh` present at all," reusable if 29a/29b need a fast tool-presence check before shelling to it:
```rust
cmd_check("gh CLI", "gh", "--version", "brew install gh / apt install gh"),
```

---

### `crates/devflow-cli/tests/release_status.rs` (NEW, 29a) / commit-point tests (29c)

**Analog:** `crates/devflow-cli/tests/release_check.rs` (entire file, 562 lines) — the direct precedent for this command family's integration tests.

**Binary-driving pattern, not internal-handler calls** (lines 1-33):
```rust
fn devflow_bin() -> &'static str {
    env!("CARGO_BIN_EXE_devflow")
}

/// Runs `devflow release <args> <project>` with an ISOLATED `HOME` (a fresh
/// empty directory, no `.gitconfig`) and no inherited `SSH_AUTH_SOCK`/
/// `SSH_AGENT_PID` — the signing-viability check reads `git config
/// gpg.format`/`user.signingkey`, which git resolves through the OPERATOR's
/// global `~/.gitconfig` even inside a throwaway fixture repo.
fn run_release(project: &Path, args: &[&str]) -> Output {
    let isolated_home = tempfile::tempdir().unwrap();
    Command::new(devflow_bin())
        .arg("release")
        .args(args)
        .arg(project)
        .env("HOME", isolated_home.path())
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("SSH_AGENT_PID")
        .output()
        .expect("spawn devflow release")
}
```
Copy this HOME-isolation discipline for any new test that touches signing/tag state. For 29a tests that hit real network oracles (crates.io, GitHub API), this pattern extends naturally to a `run_release_status` helper — but note RESEARCH.md flags that genuinely testing "Unreachable" branches requires either network-mocking or accepting these as integration-only, not unit-testable, cases; the 29a pure-logic three-way classification (given a fixed HTTP status code / git output) should be unit-tested the way `check_signing_viability`'s many `#[test]` cases are (see `git.rs`'s test module), not only through the binary.

**Real-git-fixture-repo pattern (never a mock)** (lines 35-63):
```rust
fn git(root: &Path, args: &[&str]) {
    let output = devflow_core::test_support::git_command(root)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
}

fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    git(root, &["checkout", "-q", "-b", "develop"]);
}

fn commit(root: &Path, name: &str) {
    std::fs::write(root.join(name), name).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", &format!("add {name}")]);
}
```
Note `crate::test_support::git_command` — a public re-export of the hermetic wrapper for use from `devflow-cli`'s own integration tests (outside the `devflow-core` crate boundary). New tests must use this, never `Command::new("git")` directly, mirroring the codebase-wide hermeticity discipline.

**`crates/devflow-core/src/git.rs`'s in-crate unit-test fixtures** (lines 1005-1050) are the equivalent for anything tested from *inside* `devflow-core` (e.g. `release_observe.rs`'s own `#[cfg(test)] mod tests` if colocated, or a new fixture module):
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
fn flow(root: &Path) -> GitFlow { GitFlow::new(root) }
```

**Rejection-of-invalid-invocation test pattern** (lines 127-142, `release_without_check_is_rejected`) — precedent for testing 29b/29c's own required-flag/authorization-mandate gating (e.g. a future `--yes-release` flag):
```rust
#[test]
fn release_without_check_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let output = run_release(dir.path(), &[]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "...");
    assert!(stderr.contains("DEN-50"), "...");
}
```

## Shared Patterns

### Hermetic subprocess invocation (git AND cargo)
**Source:** `crates/devflow-core/src/git.rs:61-94` (`git_command`, `hermetic_command`)
**Apply to:** `release_observe.rs`'s ls-remote/ref calls, `release_publish.rs`'s `cargo publish` calls, `git.rs`'s new signed-tag/ruleset-discovery wrappers. Every git invocation and the `cargo publish` invocation MUST go through `git_command`/`hermetic_command` — never `Command::new("git")` or `Command::new("cargo")` directly. `curl`/`gh` calls do not need env-scrubbing (git-specific vars don't redirect them) but should still pin `.current_dir(project_root)` for consistency.

### Three-valued oracle result (Present/Absent/Unreachable), never a boolean
**Source:** `crates/devflow-core/src/git.rs:533-545` (`AncestorStatus`), `758-767` (`SigningViability`)
**Apply to:** Every one of 29a's six observation functions, and every "is this step already done" pre-check inside 29b/29c. Always carry a `reason: String` on the "could not determine" arm — never a bare unit variant — so the CLI layer can report *why*, matching `SigningViability::Unknown { reason }`/`NotViable { reason }`'s existing convention.

### Check-list-then-report CLI reporting
**Source:** `crates/devflow-cli/src/commands.rs:2000-2005` (`Check` struct), `2206-2242` (`release_check`), `2008+` (`doctor`)
**Apply to:** `release_status` (29a) directly reuses this shape (same `Check` struct, same `"ok"|"warn"|"fail"` status strings, same icon-printing loop). 29b/29c's step-by-step execution reporting should follow the same "print each step's outcome, fail loud on the first hard stop" structure rather than inventing new output formatting.

### `gh` invocation discipline: auth-gate first, never log raw output
**Source:** `crates/devflow-cli/src/preflight.rs:637-657` (`preflight_gh_auth_check`)
**Apply to:** All new `gh pr create`/`gh pr merge`/`gh api repos/.../rulesets` call sites in 29b. Check `gh auth status` (or rely on an equivalent existing gate) before issuing any other `gh` command; never capture/print raw `gh` stdout/stderr into logs (T-17-13 precedent).

### `publish_order` reuse (never recompute)
**Source:** `crates/devflow-core/src/git.rs:580-606`, already consumed by `crates/devflow-cli/src/commands.rs:2343-2360` (`check_publish_order`)
**Apply to:** `release_publish.rs`'s 29c publish sequencing. Call the existing function; do not hardcode `["devflow-core", "devflow"]` as a literal anywhere.

### CLI arg/dispatch shape
**Source:** `crates/devflow-cli/src/main.rs:230-243` (`Command::Release` variant), `575-589` (dispatch arm), `CliError` enum (`main.rs:414-429`)
**Apply to:** All new `Command::Release*` variants and their dispatch arms. `#[arg(default_value = ".")] project: PathBuf` + `project_root(project)?` resolution is the established convention for every subcommand in this CLI, not specific to `release`.

## No Analog Found

| File | Role | Data Flow | Reason |
|---|---|---|---|
| crates.io HTTP client wrapper (`curl` shell-out helper, likely inside `release_observe.rs`/`release_publish.rs`) | utility | request-response (HTTP) | No existing HTTP-calling code anywhere in this codebase — every external interaction so far is `git`/`gh`/`cargo` subprocess shelling, never raw HTTP. RESEARCH.md's Pattern 1 code example (`crate_version_published`, lines 189-208 of 29-RESEARCH.md) is the closest thing to an analog and should be used as the direct template instead of a codebase precedent. |
| `gh pr create` / `gh pr merge --auto <method>` / `gh api repos/.../rulesets` call sites (29b) | service (external-process invocation) | event-driven-ish (fire PR, poll via re-observation) | No existing call site creates or merges a PR via `gh` — only `gh auth status` (preflight) and `gh --version` (doctor) exist today. Build these using RESEARCH.md's Pattern 2 (merge-method discovery + fixed policy table) and the Common Pitfalls' explicit-method-flag requirement as the template; `preflight_gh_auth_check`'s subprocess-handling shape (Ok/Ok-fail/Err three-arm match) is still the closest structural precedent even though the specific `gh` verbs are new. |
| `scripts/sync-main-to-develop.sh` port into Rust (29b, `devflow sync`) | service | CRUD (merge) | Existing logic lives in bash, not Rust — RESEARCH.md's Don't-Hand-Roll table directs porting it "almost verbatim," but there is no existing Rust analog to extract excerpts from. Read `scripts/sync-main-to-develop.sh` directly during planning/implementation rather than relying on a Rust precedent. |

## Metadata

**Analog search scope:** `crates/devflow-core/src/{git.rs,hooks.rs,version.rs}`, `crates/devflow-cli/src/{commands.rs,main.rs,preflight.rs}`, `crates/devflow-cli/tests/release_check.rs`
**Files scanned:** 7 source files + 1 test file (all read directly; no grep-only guesses)
**Pattern extraction date:** 2026-07-31
