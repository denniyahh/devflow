# Phase 12: Bootstrap + Housekeeping - Pattern Map

**Mapped:** 2026-07-08
**Files analyzed:** ~14 (2 new-subcommand areas + 9 targeted-fix locations + test-gap additions)
**Analogs found:** 12 / 14 (2 items in 12a have no analog — flagged below)

Rust workspace: `devflow-cli` (binary crate, CLI parsing + orchestration in
`crates/devflow-cli/src/main.rs`) + `devflow-core` (library crate, all
domain logic under `crates/devflow-core/src/`). `.planning/intel/API-SURFACE.md`
is empty/inapplicable (JS-only extractor) — this map is built entirely from
reading the `.rs` sources directly.

## File Classification

| New/Modified Item | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `new-project` subcommand (12a) | route (clap subcommand) + service | file-I/O | `Command::Doctor` variant + `fn doctor()` (`crates/devflow-cli/src/main.rs:148-155`, `1209`) | role-match (closest "scaffolds/reports without requiring existing `.devflow` state" command) |
| `map-codebase` subcommand (12a) | route (clap subcommand) + service | file-I/O / transform | `Command::Doctor` variant + `fn doctor()` (same as above); secondarily `Command::List`/`fn list()` (`main.rs:127-131`, `1072-1090`) for the "scan + tabular report" shape | role-match, no exact prior art |
| `save_state` atomic-write fix (WR-07) | utility (file-I/O) | CRUD (persist) | `write_atomic()` in `crates/devflow-core/src/gates.rs:193-201` | exact — reuse directly |
| `version.rs` `[[array-of-tables]]`/inline-table fix (WR-04) | utility (parser) | transform | existing `find_version_in_contents`/`replace_version_in_contents` in same file (`version.rs:182-249`) | exact (self-analog, extend in place) |
| `monitor.rs:84-116` shell-escape → `Command::new().args()` (WR-01) | service | event-driven (spawn) | existing shell-out pattern in `git.rs` `git_output()` (uses `std::process::Command::new("git").args([...])`) | role-match — codebase already has an args-vector spawn pattern to copy |
| `agent.rs:67` `libc::kill` pid_t fix (WR-02) | utility | event-driven | self-contained one-line type fix, no analog needed | n/a |
| `hooks.rs:95-104` `BranchCleanup` warn-vs-error (WR-03) | service | event-driven | `hooks.rs` `transition_map_finalizes_docs_and_changelog_before_ship` test block (`hooks.rs:213-219`) for test-style analog if a test is added alongside the fix | role-match |
| `ship.rs:380-396` doc-only clarify (WR-05) | utility | transform | n/a (docs only) | n/a |
| `main.rs:826-832` `retry_after_from_reason` fix (WR-06) | utility | transform | existing test `retry_after_from_reason_strips_prefix` (`main.rs:1451-1459`) already covers the target behavior — extend, don't reinvent | exact |
| `ship.rs:450-459` `shell_quote` char-class fix (WR-08) | utility | transform | self-contained, extend existing function in place | n/a |
| `agent_result.rs:184-195` byte vs. char reversal (WR-09) | utility | transform | self-contained; not urgent | n/a |
| `phase7_cli.rs:46-53` `write_config`/`write_last_ship` removal (WR-10) | test | request-response (CLI e2e) | rest of `phase7_cli.rs` test bodies (`run_devflow`/`fake_bin_dir` helpers, lines 12-140) | exact |
| `state.rs:46-51` remove unused `agent_result`/`agent_stdout_path` fields (IN-02) | model | CRUD | self-contained field removal | n/a |
| `AgentKind`/`Agent`/`AgentAdapter` rename (IN-03) | model/trait | n/a | `state.rs` + `agents/mod.rs` (read both before renaming — trait impls in `agents/claude.rs`, `agents/codex.rs`, `agents/opencode.rs` all reference the trait name) | exact (mechanical rename) |
| `main.rs:1175` `cargo fmt --check` (IN-04) | utility | n/a | one-line string literal fix in `fn test_cmd` (`main.rs:1171-1200`ish) | n/a |
| New test: `advance()`/`transition()` orchestration (12f) | test | request-response | `crates/devflow-cli/src/main.rs` inline `#[cfg(test)] mod tests` (`main.rs:1396-1470`) — same-module `super::*` access to private `advance`/`transition`/`handle_validate_outcome` fns | exact |
| New test: `consecutive_failures` → `MAX_CONSECUTIVE_FAILURES` end-to-end (12f) | test | request-response | `mode.rs` `auto_does_not_gate_validate_until_failure_threshold` (`mode.rs:104-109`) for the unit half; drive the *end-to-end* version through `main.rs`'s inline tests calling `handle_validate_outcome` repeatedly, mirroring `state.rs`'s `consecutive_failures_persists_across_advance_calls` (`state.rs:180-192`) for state-persistence assertions | exact |
| New test: `transition()` hook firing (12f) | test | event-driven | `hooks.rs` `transition_map_finalizes_docs_and_changelog_before_ship` (`hooks.rs:213-219`) for the map-lookup half; add an inline `main.rs` test calling `transition()` directly against a temp repo (pattern: `hooks.rs` tests' `init_repo`/`git`/`ctx` helpers, `hooks.rs:180-211`) to assert real hook side effects fire |
| New test: gate timeout at real 7-day value (12f) | test | request-response | `gates.rs` `poll_response_times_out_when_absent` (`gates.rs:256-260`) — currently only exercises `timeout_secs=0`; extend by asserting `Gates::poll_response` returns promptly once a response file is written even when `timeout_secs=GATE_TIMEOUT_SECS` (`main.rs:16`, `7*24*60*60`), proving the backoff loop doesn't block on the full deadline rather than actually sleeping 7 days |
| New test: `abort` gate path (12f) | test | event-driven | `GateAction::Abort(reason)` handling in `main.rs` (`handle_validate_outcome:414`, `handle_ship_outcome:435`); mirror `hooks.rs`/`main.rs` inline test style, drive via `run_gate` with a fake `GateResponse{approved:false, note:Some("abort: ...")}` |
| New test: `list_feature_branches` ahead/behind correctness (12f) | test | CRUD (git introspection) | `git.rs::list_feature_branches` (`git.rs:280-301`) — no existing ahead/behind unit test found; mirror `hooks.rs`/`version.rs` `init_repo`/`git`/`commit` temp-repo helper pattern to create a feature branch N commits ahead/behind `develop` and assert `BranchInfo.ahead`/`.behind` |
| New test: `version::write_version` against workspace `Cargo.toml` (12f) | test | file-I/O | `version.rs::write_version_replaces_in_cargo_toml` (`version.rs:356-375`) — exact structural analog, currently only covers `[package]`; add a sibling test with `[workspace.package]\nversion = "..."` content |
| New test: `parse_rfc3339ish` negative UTC offsets (12f) | test | transform | `ship.rs::parse_rfc3339ish` (`ship.rs:359-390`) + its existing `#[cfg(test)]` block (search `mod tests` further down `ship.rs`) — only `Z`/UTC offsets tested; add cases like `"2026-06-18T15:45:30-05:00"` |
| New test: monitor behavior on `devflow advance` failure (missing/corrupt state) (12f) | test | event-driven | `crates/devflow-core/tests/monitor_e2e.rs` (full-file integration style) and `monitor.rs`'s inline `#[cfg(test)] mod tests` (`monitor.rs:160-258`) |

## Pattern Assignments

### WR-07 — `workflow.rs::save_state` atomic-write fix

**Analog:** `crates/devflow-core/src/gates.rs:193-201` (`write_atomic`, already `pub(crate)`-visible... actually private `fn` in `gates.rs` module)

Current buggy code (`crates/devflow-core/src/workflow.rs:32-39`):
```rust
pub fn save_state(state: &State) -> Result<(), WorkflowError> {
    debug!("saving state: phase={} stage={}", state.phase, state.stage);
    let dir = devflow_dir(&state.project_root);
    std::fs::create_dir_all(&dir)?;
    let contents = serde_json::to_string_pretty(state)?;
    std::fs::write(dir.join("state.json"), contents)?;   // <-- not atomic
    Ok(())
}
```

Analog to copy (`gates.rs:191-201`):
```rust
/// Write `contents` to `path` atomically: write a temp file in the same
/// directory, then rename over the target so readers never see a partial write.
fn write_atomic(path: &Path, contents: &str) -> Result<(), GateError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
```
`write_atomic` in `gates.rs` is a private, `GateError`-typed helper — it cannot
be imported directly into `workflow.rs` without either (a) making it `pub(crate)`
and generalizing its error type, or (b) duplicating the same 6-line
temp-write-then-rename shape locally in `workflow.rs` typed to `WorkflowError`
(which already has an `Io(#[from] std::io::Error)` variant, so `?` composes
cleanly). Recommend duplicating the shape locally in `workflow.rs` (same
tmp-then-rename idiom, `.with_extension("tmp")`) rather than cross-module
coupling two unrelated error enums — matches the "surgical, minimal" project
style already used by `gates.rs`.

### 12a — New CLI subcommands (`new-project`, `map-codebase`)

**Analog:** `Command::Doctor` variant + dispatch arm + `fn doctor()`

Subcommand declaration pattern (`crates/devflow-cli/src/main.rs:147-155`):
```rust
/// Audit the environment and report what's installed, missing, or broken.
Doctor {
    /// Output as JSON.
    #[arg(long)]
    json: bool,
    /// Project root (optional — doctor works without a project too).
    #[arg(default_value = ".")]
    project: PathBuf,
},
```

Dispatch arm (`main.rs:243`):
```rust
Command::Doctor { json, project } => doctor(&project_root(project)?, json),
```

Simpler "resolve project root, do file-I/O, print report" handler shape to
mirror, `fn list()` (`main.rs:1072-1090`):
```rust
fn list(project_root: &Path) -> Result<(), CliError> {
    let git = GitFlow::new(project_root);
    let branches = git.list_feature_branches()?;
    if branches.is_empty() {
        println!("no open feature branches");
        return Ok(());
    }
    println!("{:<25} {:>6} {:>7}  LAST COMMIT", "BRANCH", "AHEAD", "BEHIND");
    for b in &branches {
        println!("{:<25} {:>6} {:>7}  {}", b.name, b.ahead, b.behind, b.last_commit);
    }
    Ok(())
}
```

Project-root resolution helper to reuse as-is (`main.rs:1112-1123`):
```rust
fn project_root(project: PathBuf) -> Result<PathBuf, CliError> {
    if project.exists() {
        project.canonicalize()
            .map_err(|err| CliError::Message(format!("failed to resolve project path: {err}")))
    } else {
        Err(CliError::Message(format!("project path does not exist: {}", project.display())))
    }
}
```
Note: for `new-project`, the target directory will NOT yet exist (that's the
point of scaffolding), so `project_root()`'s `.exists()` guard is inverted from
what `new-project` needs — do not reuse it verbatim for that subcommand; every
other existing command assumes the project already exists.

Error enum extension pattern — new error variants for filesystem/scaffolding
failures should follow the existing `#[error(transparent)]` + `#[from]` shape
in `CliError` (`main.rs:158-176`).

**No analog found:** neither `new-project` scaffolding (writing template files
into a fresh directory) nor `map-codebase` (static analysis / dependency
graph over an existing codebase) has any prior art in this codebase — every
existing subcommand assumes an established `.devflow`-tracked git repo.
Planner should treat these as greenfield, using only the clap/dispatch/error
conventions above.

### 12f — `version::write_version` workspace Cargo.toml test

**Analog:** `crates/devflow-core/src/version.rs:356-375` (`write_version_replaces_in_cargo_toml`)
```rust
#[test]
fn write_version_replaces_in_cargo_toml() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nversion = \"0.1.0\"\n",
    ).unwrap();
    let path = write_version(dir.path(), &Version { major: 2, minor: 3, patch: 4 }).unwrap();
    let contents = std::fs::read_to_string(&path).unwrap();
    assert!(contents.contains("version = \"2.3.4\""));
}
```
New sibling test: same shape, seed file with `"[workspace.package]\nversion = \"0.1.0\"\nedition = \"2024\"\n"` instead — proves `field_for`/`replace_version_in_contents` correctly targets `workspace.package.version` (see `read_major_from_workspace_package`, `version.rs:304-314`, for the existing "workspace section" detection precedent to mirror).

### 12f — `parse_rfc3339ish` negative offset test

**Analog:** existing `ship.rs` test module covering `parse_rfc3339ish` (function at `ship.rs:359-390+`; its `split_time_and_offset` helper already parses a `+`/`-` offset — grep confirms offset handling exists in source, just untested for negative values). Add a case such as `assert!(parse_rfc3339ish("2026-06-18T10:45:30-05:00").is_some())` plus an assertion on the resulting UTC-normalized fields, mirroring whatever assertion style the existing `Z`-suffix test uses in the same `mod tests` block.

### 12f — Gate timeout at real 7-day value

**Analog:** `crates/devflow-core/src/gates.rs:256-260`
```rust
#[test]
fn poll_response_times_out_when_absent() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Gates::poll_response(dir.path(), 11, Stage::Ship, 0).is_none());
}
```
And the "returns when file appears" analog (`gates.rs:241-254`) shows the
write-then-poll shape. New test should reuse `GATE_TIMEOUT_SECS` (defined in
`crates/devflow-cli/src/main.rs:16` as `7 * 24 * 60 * 60`, but `gates.rs` is
`devflow-core` — either re-derive the constant locally in the test or expose
it from `devflow-core` if not already) and assert the poll returns
**immediately** (well under a test timeout) once a response file is written,
proving the backoff loop's early-return-on-file-found path works at the real
deadline value without actually sleeping — do not literally sleep 7 days in CI.

### 12f — `advance()`/`transition()` orchestration tests

**Analog:** `crates/devflow-cli/src/main.rs:1396-1470` (existing inline
`#[cfg(test)] mod tests { use super::*; ... }`) gives direct, same-crate
access to the private functions under test (`advance`, `transition`,
`handle_validate_outcome`, `handle_ship_outcome` are all private `fn`s in the
same file, `main.rs:338-457`). Combine with the temp-git-repo helper pattern
from `hooks.rs:180-211` (`git()`, `init_repo()`, `ctx()`) to build a real
`project_root` with `.devflow/state.json` seeded via `workflow::save_state`,
then call `advance(&project_root)` directly and assert on the resulting
`state.stage`/`consecutive_failures` after reload via `workflow::load_state`.

### WR-01 — `monitor.rs` shell-script → `Command::new().args()`

**Analog:** `git.rs`'s subprocess-spawn pattern already uses an args-vector
(not shell-interpolated) form — follow that same `std::process::Command::new(program).args([...])` shape when replacing the shell-script generation in `monitor.rs:84-116`, eliminating the `shell_escape` injection surface entirely as the CONTEXT.md finding suggests.

## Shared Patterns

### Temp-repo test helpers (git-backed unit tests)
**Source:** `crates/devflow-core/src/version.rs:256-280`, `crates/devflow-core/src/hooks.rs:178-202`
**Apply to:** every new 12f test that needs a real git repo (`list_feature_branches`, `transition()` hook firing, `advance()` orchestration)
```rust
fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git").args(args).current_dir(root).output().unwrap().status.success();
    assert!(ok, "git {args:?} failed");
}
fn init_repo(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "tag.gpgsign", "false"]);
    git(root, &["config", "core.hooksPath", "/dev/null"]);
}
```

### Atomic file writes
**Source:** `crates/devflow-core/src/gates.rs:193-201` (`write_atomic`)
**Apply to:** `workflow.rs::save_state` (WR-07) — copy the tmp-write-then-rename idiom; keep it local to `workflow.rs` typed against `WorkflowError` rather than importing `gates.rs`'s private, `GateError`-typed helper across modules.

### CLI subcommand declaration + dispatch
**Source:** `crates/devflow-cli/src/main.rs:29-156` (`enum Command` variants) and `main.rs:195-243` (`match cli.command { ... }` dispatch)
**Apply to:** `new-project`, `map-codebase` (12a) — add variants to `enum Command`, add dispatch arms, add handler `fn`s following the `fn doctor()`/`fn list()` shape (resolve `project_root`, do the work, print a plain-text report, return `Result<(), CliError>`).

### CLI error propagation
**Source:** `crates/devflow-cli/src/main.rs:158-176` (`enum CliError`)
**Apply to:** any new subcommand or fixed function that can fail — add `#[error(transparent)] X(#[from] devflow_core::x::XError)` variants rather than stringly-typed errors, matching existing style.

## No Analog Found

| File/Item | Role | Data Flow | Reason |
|---|---|---|---|
| `new-project` scaffolding logic | service | file-I/O | No existing "create fresh project from template" code in this codebase — closest is `Doctor`'s environment-probing shape, but the actual file-generation logic is greenfield |
| `map-codebase` analysis logic | service | transform | No existing static-analysis/dependency-graph code in this codebase; closest analog is only the CLI plumbing (`fn list()`/`fn doctor()` shape), not the analysis itself |

## Metadata

**Analog search scope:** `crates/devflow-cli/src/main.rs`, `crates/devflow-cli/tests/phase7_cli.rs`, `crates/devflow-core/src/{gates,workflow,version,mode,stage,state,hooks,git,ship,monitor,agent_result}.rs`, `crates/devflow-core/tests/monitor_e2e.rs`
**Files scanned:** 17 `.rs` files (full workspace minus `agents/*.rs` adapters and `lib.rs`/`config.rs`/`lock.rs`/`recover.rs`/`worktree.rs`, which were grepped but not read in full — no phase-12 items map to them)
**Pattern extraction date:** 2026-07-08
