//! Build staleness/provenance (17d/18c, shaped by 18-06): deciding whether
//! the running `devflow` binary is stale relative to the tree under test
//! (the ancestry half of D-19's composite check plus the live dirty-tree
//! arm, CR-02/17-11), and enforcing that decision as a hard self-dogfood
//! block or a soft warning (D-17/D-18, 18c worktree-aware execution root).
//! Extracted mechanically (19-07, D-09 pure move) out of `main.rs` — every
//! function below is byte-identical to its pre-move body modulo an added
//! `pub(crate)` and adjusted `use` paths.

use devflow_core::events;
use devflow_core::gates;
use devflow_core::state::State;
use std::path::Path;

use crate::CliError;

/// Whether the build embedded in `embedded_commit` is stale relative to
/// `execution_root`'s current `HEAD` — the tree where the code under test
/// actually lives (18c: the phase's worktree when one is set, else
/// `project_root` — see `enforce_build_staleness`) — the ancestry half of
/// D-19's composite definition. Per git's documented exit-code contract for
/// `merge-base --is-ancestor` (exit 0 = ancestor, exit 1 = not, other =
/// error/unknown commit — Pitfall 4), exit 1 is treated as definitively
/// Stale; any other outcome (including an empty `embedded_commit` — D-20:
/// absence of provenance is not staleness) is Indeterminate, never a false
/// block. WR-01 (17-06 gap closure): exit 0 alone is NOT sufficient for
/// Fresh — `merge-base --is-ancestor` also exits 0 when `embedded_commit` is
/// a STRICT ancestor of HEAD (HEAD moved forward since the build), which is
/// exactly the "committed new commits, forgot to rebuild" incident class
/// this fix closes. Only an EXACT match to the current HEAD commit is
/// genuinely Fresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Staleness {
    Fresh,
    Stale,
    /// The embedded commit is a strict DESCENDANT of `execution_root`'s
    /// HEAD: the binary is newer than the source it drives. Not the
    /// "committed, forgot to rebuild" incident this gate exists to catch,
    /// so it never blocks — but it is still a build/source mismatch worth
    /// surfacing.
    Ahead,
    Indeterminate,
}

fn embedded_commit_is_stale(execution_root: &Path, embedded_commit: &str) -> Staleness {
    if embedded_commit.is_empty() {
        return Staleness::Indeterminate;
    }
    let output = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", embedded_commit, "HEAD"])
        .current_dir(execution_root)
        .output();
    match output.map(|o| o.status.code()) {
        Ok(Some(0)) => match run_git_stdout(execution_root, &["rev-parse", "HEAD"]) {
            Some(head) if head.trim() == embedded_commit.trim() => Staleness::Fresh,
            Some(_) => Staleness::Stale,
            None => Staleness::Indeterminate,
        },
        // Exit 1 only says "not an ancestor" — which is true both for a
        // genuinely older/divergent commit AND for a descendant. Probe the
        // reverse direction to tell them apart, or an ahead build gets
        // reported as stale and hard-blocked.
        Ok(Some(1)) => {
            let reverse = std::process::Command::new("git")
                .args(["merge-base", "--is-ancestor", "HEAD", embedded_commit])
                .current_dir(execution_root)
                .output();
            match reverse.map(|o| o.status.code()) {
                Ok(Some(0)) => Staleness::Ahead,
                Ok(Some(1)) => Staleness::Stale,
                _ => Staleness::Indeterminate,
            }
        }
        _ => Staleness::Indeterminate,
    }
}

/// Shell `git` in `project_root`, returning `None` on any failure (missing
/// binary, non-git directory, non-zero exit) — same argv-array idiom as
/// `build.rs`'s `run_git`.
pub(crate) fn run_git_stdout(project_root: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

/// The live half of D-19's composite staleness (CR-02, 17-11): whether
/// `execution_root`'s working tree — the tree where the code under test
/// actually lives (18c) — CURRENTLY has any tracked, modified file that can
/// change the compiled binary (`affects_compiled_binary`, reused from
/// 17-10 — not duplicated). No timestamp is available any more (`build.rs`
/// no longer embeds one — CR-02), so this cannot itself distinguish
/// "modified after the build" from "modified before the build, still
/// uncommitted"; combined with the build's own `build_dirty` flag in
/// `combined_staleness`, it distinguishes "built clean, source changed
/// since" (definitely Stale) from "built dirty, source still dirty"
/// (Indeterminate — cannot tell "same dirt" from "more dirt" without a
/// timestamp, Pitfall 4). Returns `None` when git itself is unavailable, so
/// the composite check falls back to the ancestry arm alone.
fn tree_has_modified_build_inputs(execution_root: &Path) -> Option<bool> {
    let status = run_git_stdout(execution_root, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Some(false);
    }
    // WR-03: enumerate from `--porcelain` itself rather than `git ls-files -m`.
    // `ls-files -m` compares worktree-vs-INDEX, so a *staged* source edit
    // (`git add src/lib.rs`) reports nothing while porcelain reports `M `.
    // That fell through to the ancestry arm as Fresh, letting a stale binary
    // drive its own workspace — the exact false-evidence class this gate exists
    // to catch. Untracked files stay excluded, as under `ls-files -m`.
    Some(
        status
            .lines()
            .any(|line| porcelain_tracked_path(line).is_some_and(affects_compiled_binary)),
    )
}

/// The repo-relative path a `git status --porcelain` line refers to, or `None`
/// for untracked (`??`) entries. Porcelain v1 lines are `XY<space>PATH`, with
/// renames/copies rendered as `ORIG -> PATH`; the destination is the path that
/// exists in the worktree. Paths containing special characters are quoted by
/// git, so surrounding quotes are stripped.
fn porcelain_tracked_path(line: &str) -> Option<&str> {
    if line.len() < 4 || line.starts_with("??") {
        return None;
    }
    let path = &line[3..];
    let path = path.rsplit(" -> ").next().unwrap_or(path);
    Some(path.trim_matches('"'))
}

/// Whether a repo-relative path can change the compiled binary. The live
/// dirty-tree arm of the staleness check must consider ONLY these: a dirty
/// `CHANGELOG.md` or `.planning/` file says nothing about whether the
/// binary matches its source.
///
/// Found live — DevFlow's own `ChangelogAppend` hook dirtied `CHANGELOG.md`
/// during the Validate→Ship transition, which an unfiltered check read as
/// a stale build, hard-blocking Ship on a file the pipeline had just written.
fn affects_compiled_binary(rel_path: &str) -> bool {
    const BUILD_AFFECTING_FILES: [&str; 4] = [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "rust-toolchain.toml",
    ];
    rel_path.ends_with(".rs")
        || BUILD_AFFECTING_FILES
            .iter()
            .any(|name| rel_path == *name || rel_path.ends_with(&format!("/{name}")))
}

/// D-19: composite staleness (CR-02, 17-11: the dirty-flag arm replaces the
/// old mtime arm; the ancestry arm below is unchanged). Evaluates
/// `execution_root` — the tree where the code under test actually lives
/// (18c). Decision table for the second signal, evaluated only once
/// ancestry alone hasn't already settled Stale:
///
/// | build was dirty | tree has modified build inputs now | result |
/// |---|---|---|
/// | `false` | yes | **Stale** — built clean, source changed since (CR-02) |
/// | `true` | yes | **Indeterminate** — can't distinguish "same dirt" from |
/// |         |     | "more dirt" without a timestamp; warn, never block |
/// |         |     | (Pitfall 4) |
/// | either | no | fall through to the ancestry result unchanged |
fn combined_staleness(
    execution_root: &Path,
    embedded_commit: &str,
    build_dirty: bool,
) -> Staleness {
    let ancestry = embedded_commit_is_stale(execution_root, embedded_commit);
    if ancestry == Staleness::Stale {
        return Staleness::Stale;
    }
    match tree_has_modified_build_inputs(execution_root) {
        Some(true) if build_dirty => Staleness::Indeterminate,
        Some(true) => Staleness::Stale,
        _ => ancestry,
    }
}

/// D-17: whether `project_root` IS the DevFlow workspace itself (as opposed
/// to some other project being driven by a devflow binary) — deterministic,
/// offline, no config. Scans the `members = [...]` array of the root
/// `Cargo.toml` for BOTH exact member-path strings, never a package `name`
/// (the CLI crate's package is named `devflow`, not `devflow-cli` — a name
/// match would never fire on the incident workspace; review consensus #2 +
/// Plan 05 MEDIUM OpenCode). No TOML parser is used here: locating the
/// `members` array's bounds first, then scanning within it, is the
/// sanctioned middle ground and is unlikely to false-positive on an
/// unrelated project.
fn is_self_dogfood_workspace(project_root: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(project_root.join("Cargo.toml")) else {
        return false;
    };
    // WR-05: anchor on the `members` KEY, not the first substring hit.
    // `default-members` contains `members`, so a bare `find` would scan that
    // array instead and silently degrade the self-dogfood hard block to a
    // warning the moment the root manifest gains a `default-members` key
    // above `members`.
    let Some(members_start) = contents.match_indices("members").find_map(|(idx, _)| {
        let preceded_by_ident = contents[..idx]
            .chars()
            .next_back()
            .is_some_and(|ch| ch.is_alphanumeric() || ch == '_' || ch == '-');
        (!preceded_by_ident).then_some(idx)
    }) else {
        return false;
    };
    let rest = &contents[members_start..];
    let Some(open_rel) = rest.find('[') else {
        return false;
    };
    let after_open = &rest[open_rel + 1..];
    let Some(close_rel) = after_open.find(']') else {
        return false;
    };
    let members = &after_open[..close_rel];
    // WR-02: compare each array element for exact equality rather than
    // substring-matching the whole array. `str::contains` would classify a
    // workspace whose members are `crates/devflow-core-extras` /
    // `crates/devflow-cli-plugin` as self-dogfood, and self-dogfood + Stale
    // hard-blocks the pipeline — the one outcome this must never inflict on
    // an unrelated project.
    let has_member = |wanted: &str| {
        members
            .split(',')
            .any(|entry| entry.trim().trim_matches(['"', '\'']).trim() == wanted)
    };
    has_member("crates/devflow-core") && has_member("crates/devflow-cli")
}

/// The outcome of the self-dogfood staleness gate (D-18): `Block` only when
/// the project IS DevFlow's own workspace AND its build is confirmed Stale —
/// everything else (an ordinary project, or an Indeterminate result on any
/// project, Pitfall 4) only warns or is silent. Kept pure so the
/// self-dogfood-blocks vs. ordinary-warns split is directly unit-testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StalenessOutcome {
    Block,
    Warn,
    Ok,
}

fn staleness_outcome(is_self_dogfood: bool, staleness: Staleness) -> StalenessOutcome {
    match (is_self_dogfood, staleness) {
        (true, Staleness::Stale) => StalenessOutcome::Block,
        (false, Staleness::Stale) => StalenessOutcome::Warn,
        (_, Staleness::Ahead) => StalenessOutcome::Warn,
        (_, Staleness::Indeterminate) => StalenessOutcome::Warn,
        (_, Staleness::Fresh) => StalenessOutcome::Ok,
    }
}

/// D-17/D-18/D-19 (17d), execution_root (18c): the self-dogfood
/// build-staleness gate, called from `launch_stage` before
/// `monitor::spawn_monitor`. A Stale build against DevFlow's OWN workspace
/// is a hard block — deliberately NOT an approvable gate, because approving
/// it would reintroduce the exact Phase 16 false-evidence incident — but it
/// is never SILENT: notify + an event fire before the blocking error is
/// returned, so an unattended cron run still sees it (reconciling D-15's
/// never-silent idiom with D-18's hard block). An ordinary project (or an
/// Indeterminate result) only warns and proceeds.
///
/// 18c: ancestry/dirty-tree checks run against `execution_root` — the
/// phase's worktree when `state.worktree_path` is set, else `project_root`
/// — because that is the tree where the code under test actually lives.
/// Evaluating a worktree-based phase against `project_root` alone is Round
/// 4 CR-01's root cause: a binary behind the worktree branch can still be a
/// descendant of `project_root`'s HEAD and misclassify `Ahead` (warn only).
///
/// `is_self_dogfood_workspace` deliberately stays anchored on `project_root`
/// (Assumption A3, 18-RESEARCH.md Pitfall 4): it answers "is this workspace
/// DevFlow's own repo at all", not "is the binary stale relative to tree X"
/// — DevFlow's bookkeeping (`.planning/`, `.devflow/`) always lives in the
/// main checkout even when execution does not, and `events::emit` keeps
/// writing there too. A git worktree shares the same tracked files as the
/// commit it is checked out to, so in practice both roots agree; the
/// residual risk is a PLAN that modified the root `Cargo.toml`'s `members`
/// array on the feature branch mid-flight, making the two roots disagree.
pub(crate) fn enforce_build_staleness(
    project_root: &Path,
    state: &State,
    embedded_commit: &str,
    build_dirty: bool,
) -> Result<(), CliError> {
    let execution_root = state.worktree_path.as_deref().unwrap_or(project_root);
    let staleness = combined_staleness(execution_root, embedded_commit, build_dirty);
    let self_dogfood = is_self_dogfood_workspace(project_root);
    match staleness_outcome(self_dogfood, staleness) {
        StalenessOutcome::Block => {
            let message = format!(
                "self-dogfood stale build blocked for stage {}: this devflow binary's \
                 embedded commit is not an ancestor of {}'s current HEAD (or its tracked \
                 source is newer than the build) — rebuild devflow before driving its own \
                 workspace (D-18; the Phase 16 false-evidence incident){}",
                state.stage,
                execution_root.display(),
                if state.worktree_path.is_some() {
                    " — evaluated against this phase's WORKTREE HEAD, not the main checkout; \
                     rebuild and reinstall the binary before resuming"
                } else {
                    ""
                }
            );
            gates::fire_gate_notify(state.phase, state.stage, &message, true);
            // WR-02 (18-fix): `message` embeds `execution_root.display()` —
            // an absolute filesystem path (and, on a typical Linux/macOS
            // path, the operator's OS username). `fire_gate_notify` and the
            // returned `Err` below are the only places that path-bearing
            // string is allowed to reach — `events::emit` persists to
            // `.devflow/events.jsonl`, which `OPERATIONS.md` advertises as
            // safe to "tail from any tool", so it must never carry a path.
            // A bare, path-free label plus the two structured facts an
            // operator actually needs (which stage, and whether a worktree
            // was involved) are enough to explain the event without leaking
            // anything.
            events::emit(
                project_root,
                state.phase,
                "self_dogfood_stale_blocked",
                serde_json::json!({
                    "stage": state.stage.to_string(),
                    "reason": "stale_build_blocked",
                    "worktree": state.worktree_path.is_some(),
                }),
            );
            Err(CliError::Message(message))
        }
        StalenessOutcome::Warn => {
            println!(
                "warning: build provenance staleness check did not confirm a fresh build for \
                 stage {} — proceeding (only DevFlow's own workspace is ever hard-blocked, D-18)",
                state.stage
            );
            Ok(())
        }
        StalenessOutcome::Ok => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use devflow_core::mode::Mode;
    use devflow_core::stage::Stage;
    use devflow_core::state::AgentKind;
    use std::path::PathBuf;

    /// D-17: matches only when BOTH exact member paths appear inside the
    /// `members = [...]` array — never a package `name` match.
    #[test]
    fn is_self_dogfood_workspace_matches_both_member_paths_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\n    \"crates/devflow-core\",\n    \"crates/devflow-cli\",\n]\n",
        )
        .unwrap();
        assert!(is_self_dogfood_workspace(root));

        let name_only = tempfile::tempdir().unwrap();
        std::fs::write(
            name_only.path().join("Cargo.toml"),
            "[package]\nname = \"devflow-cli\"\n",
        )
        .unwrap();
        assert!(
            !is_self_dogfood_workspace(name_only.path()),
            "a package NAME match must never fire — the CLI package is named `devflow`"
        );

        let partial = tempfile::tempdir().unwrap();
        std::fs::write(
            partial.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\"]\n",
        )
        .unwrap();
        assert!(!is_self_dogfood_workspace(partial.path()));

        let missing = tempfile::tempdir().unwrap();
        assert!(!is_self_dogfood_workspace(missing.path()));
    }

    /// WR-02: member paths that merely *contain* the real member names must
    /// not classify an unrelated workspace as self-dogfood — that combination
    /// hard-blocks the project's entire pipeline when its build reads Stale.
    #[test]
    fn is_self_dogfood_workspace_requires_exact_member_paths_not_substrings() {
        let lookalike = tempfile::tempdir().unwrap();
        std::fs::write(
            lookalike.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\n    \"crates/devflow-core-extras\",\n    \"crates/devflow-cli-plugin\",\n]\n",
        )
        .unwrap();
        assert!(
            !is_self_dogfood_workspace(lookalike.path()),
            "`devflow-core-extras`/`devflow-cli-plugin` are not the real members — \
             a substring match here would hard-block an unrelated project"
        );

        let prefixed = tempfile::tempdir().unwrap();
        std::fs::write(
            prefixed.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\n    \"vendor/crates/devflow-core\",\n    \"vendor/crates/devflow-cli\",\n]\n",
        )
        .unwrap();
        assert!(
            !is_self_dogfood_workspace(prefixed.path()),
            "vendored copies at a different path are not DevFlow's own workspace"
        );
    }

    /// WR-05: `"default-members"` contains `"members"`. A bare
    /// `contents.find("members")` locks onto that key's array instead, so the
    /// real member list is never scanned and the self-dogfood hard block
    /// silently degrades to a warning — with every existing test still green,
    /// because their fixtures all put `members = [...]` first.
    #[test]
    fn is_self_dogfood_workspace_anchors_on_members_not_default_members() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[workspace]\n\
             default-members = [\"crates/devflow-cli\"]\n\
             members = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();
        assert!(
            is_self_dogfood_workspace(dir.path()),
            "a `default-members` key ahead of `members` must not hide the real \
             member list — that turns the D-18 hard block into a warning"
        );
    }

    /// Build a real `git worktree add` fixture for the 18c wrong-tree defect
    /// (Round 4 CR-01): a `develop` branch with one commit (the "embedded"
    /// commit, recorded before the worktree diverges) at
    /// `<tempdir>/project`, and a feature-branch worktree checked out from
    /// it as a SIBLING directory at `<tempdir>/worktree` — deliberately NOT
    /// nested under `project`, so a test can assert unambiguously on which
    /// of the two paths a message names (a nested worktree path would
    /// contain `project_root`'s path as a string prefix, making "worktree
    /// path present" and "project_root path absent" mutually exclusive
    /// assertions). Two further commits are made INSIDE the worktree, each
    /// touching a `.rs` file (build-affecting), so `project_root`'s HEAD
    /// never moves and the worktree's HEAD advances two commits past the
    /// recorded hash. Mirrors
    /// `worktree::tests::add_creates_worktree_on_new_branch`'s construction
    /// (`git worktree add -b <branch> <path> <start_point>`) — the closest
    /// existing precedent for a real worktree fixture.
    ///
    /// Returns `(tempdir_guard, worktree_path, embedded_commit)`.
    /// `project_root` is `tempdir_guard.path().join("project")`. The guard
    /// must be kept alive for the duration of the test.
    fn worktree_staleness_fixture() -> (tempfile::TempDir, PathBuf, String) {
        let outer = tempfile::tempdir().unwrap();
        let project_root = outer.path().join("project");
        std::fs::create_dir_all(&project_root).unwrap();
        let worktree_path = outer.path().join("worktree");

        let git = |args: &[&str], cwd: &Path| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(cwd)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} in {cwd:?} failed"
            );
        };

        git(&["init", "-q", "-b", "develop"], &project_root);
        git(&["config", "user.email", "t@e.st"], &project_root);
        git(&["config", "user.name", "t"], &project_root);
        git(&["config", "commit.gpgsign", "false"], &project_root);
        git(&["config", "core.hooksPath", "/dev/null"], &project_root);
        std::fs::create_dir_all(project_root.join("src")).unwrap();
        std::fs::write(project_root.join("src/lib.rs"), "// base\n").unwrap();
        git(&["add", "."], &project_root);
        git(&["commit", "-q", "-m", "base"], &project_root);
        let embedded_commit = run_git_stdout(&project_root, &["rev-parse", "HEAD"])
            .expect("rev-parse HEAD")
            .trim()
            .to_string();

        git(
            &[
                "worktree",
                "add",
                "-b",
                "feature/phase-90",
                worktree_path.to_str().unwrap(),
                "develop",
            ],
            &project_root,
        );

        // Two build-affecting commits, made ONLY inside the worktree —
        // project_root's HEAD (develop) never moves. This asymmetry (Fresh
        // against project_root, Stale against the worktree) is exactly the
        // Round 4 CR-01 mechanism.
        std::fs::write(worktree_path.join("src/lib.rs"), "// wt commit 1\n").unwrap();
        git(&["add", "."], &worktree_path);
        git(&["commit", "-q", "-m", "wt commit 1"], &worktree_path);
        std::fs::write(worktree_path.join("src/lib.rs"), "// wt commit 2\n").unwrap();
        git(&["add", "."], &worktree_path);
        git(&["commit", "-q", "-m", "wt commit 2"], &worktree_path);

        (outer, worktree_path, embedded_commit)
    }

    /// 18c (Round 4 CR-01 root cause): the SAME embedded commit is
    /// simultaneously `Fresh` against `project_root` and `Stale` against the
    /// worktree HEAD. Evaluating a worktree-based phase against
    /// `project_root` alone is exactly the bug — a binary two commits behind
    /// the worktree branch reads as if it were built from the current
    /// source. Both halves are asserted in one test: a single assertion
    /// would pass for the wrong reason if the fixture were built
    /// incorrectly.
    ///
    /// This test is already GREEN pre-fix — both calls are already
    /// parameterized by a root, so this proves the fixture is correct, not
    /// that the defect is fixed. The RED proof of the actual defect (the
    /// real entry point, `enforce_build_staleness`, evaluated against the
    /// wrong root) lives in
    /// `enforce_build_staleness_blocks_self_dogfood_behind_worktree_head`.
    ///
    /// (18-fix) `worktree_staleness_fixture` spawns real `git` subprocesses
    /// unguarded — under concurrent load this raced this file's
    /// PATH-mutating tests (the same `ENV_MUTEX`/19i flake class as
    /// `transition_resets_infra_failures`), reproduced at roughly 1-in-8 to
    /// 1-in-10. Guarded under `ENV_MUTEX` so it never runs concurrently with
    /// a PATH mutator, mirroring the established pattern rather than
    /// inventing a new one.
    #[test]
    fn embedded_commit_is_stale_uses_worktree_head() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let (outer, worktree_path, embedded_commit) = worktree_staleness_fixture();
        let project_root = outer.path().join("project");

        assert_eq!(
            embedded_commit_is_stale(&project_root, &embedded_commit),
            Staleness::Fresh,
            "project_root's HEAD never moved, so the embedded commit is still an exact match"
        );
        assert_eq!(
            embedded_commit_is_stale(&worktree_path, &embedded_commit),
            Staleness::Stale,
            "the worktree branch advanced two commits past the embedded commit — Round 4 \
             CR-01's mechanism: evaluated against the wrong tree, this same commit reads Fresh"
        );
    }

    /// 18c GREEN: `enforce_build_staleness` now evaluates ancestry against
    /// the worktree HEAD (via `execution_root`) rather than `project_root`,
    /// so a self-dogfood binary behind the worktree branch is a hard
    /// BLOCK — closing Round 4 CR-01, where the identical scenario
    /// evaluated against `project_root` alone classified `Ahead` (warn
    /// only) because the embedded commit was still a descendant of
    /// `develop`.
    ///
    /// (18-fix) Guarded under `ENV_MUTEX`, same rationale as
    /// `embedded_commit_is_stale_uses_worktree_head` — this test also drives
    /// `worktree_staleness_fixture`'s unguarded real `git` subprocesses.
    #[test]
    fn enforce_build_staleness_blocks_self_dogfood_behind_worktree_head() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let (outer, worktree_path, embedded_commit) = worktree_staleness_fixture();
        let project_root = outer.path().join("project");
        std::fs::write(
            project_root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();
        assert!(is_self_dogfood_workspace(&project_root));

        let phase = 90;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, project_root.clone());
        state.stage = Stage::Code;
        state.worktree_path = Some(worktree_path.clone());

        let err =
            enforce_build_staleness(&project_root, &state, &embedded_commit, false).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains(&worktree_path.display().to_string()),
            "block message must name the worktree that was actually evaluated: {message}"
        );
        assert!(
            !message.contains(&project_root.display().to_string()),
            "block message must not name project_root when a worktree was evaluated: {message}"
        );

        // WR-02 (18-fix): the persisted event's `worktree` flag mirrors
        // `state.worktree_path.is_some()`, path-free.
        let last = devflow_core::events::last_event_for_phase(&project_root, phase)
            .expect("staleness block must record an event before returning the error");
        assert_eq!(last["reason"], "stale_build_blocked");
        assert_eq!(last["worktree"], true);
    }

    /// 18c (T-18-26): the SAME fixture with `worktree_path: None` must fall
    /// back to `project_root` and produce `Ok` — proving the
    /// `unwrap_or(project_root)` fallback preserves existing behavior for
    /// non-worktree phases and that this fix cannot start blocking them.
    ///
    /// (18-fix) Guarded under `ENV_MUTEX`, same rationale as
    /// `embedded_commit_is_stale_uses_worktree_head` — this test also drives
    /// `worktree_staleness_fixture`'s unguarded real `git` subprocesses.
    #[test]
    fn staleness_without_worktree_is_unchanged() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let (outer, _worktree_path, embedded_commit) = worktree_staleness_fixture();
        let project_root = outer.path().join("project");
        std::fs::write(
            project_root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();

        let phase = 91;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, project_root.clone());
        state.stage = Stage::Code;
        assert!(
            state.worktree_path.is_none(),
            "fixture precondition: no worktree recorded on this state"
        );

        assert!(
            enforce_build_staleness(&project_root, &state, &embedded_commit, false).is_ok(),
            "no worktree recorded must fall back to project_root, which the fixture never \
             advances past embedded_commit"
        );
    }

    /// Build a repo with a `base` commit, a diverged `side`-branch commit
    /// that is NOT an ancestor of the final `trunk` HEAD, then return to
    /// `trunk` — exercises all three `embedded_commit_is_stale` outcomes
    /// against a real git history.
    fn init_repo_with_diverged_commit(root: &Path) -> (String, String) {
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        let rev_parse = || {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        git(&["init", "-q", "-b", "trunk"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("a.txt"), "one").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "base"]);
        let base = rev_parse();

        git(&["checkout", "-q", "-b", "side"]);
        std::fs::write(root.join("side.txt"), "s").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "side"]);
        let side = rev_parse();

        git(&["checkout", "-q", "trunk"]);
        std::fs::write(root.join("trunk2.txt"), "t2").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "trunk2"]);

        (base, side)
    }

    /// Pitfall 4 / WR-01: exit 1 -> Stale, and anything else (unknown
    /// commit, empty embedded commit) -> Indeterminate, never a false block.
    /// Exit 0 (merge-base --is-ancestor) splits further: a strict ancestor
    /// of HEAD -> Stale (WR-01 fix — `base` here is an ancestor of the
    /// fixture's final `trunk2` HEAD but is NOT HEAD itself, which is
    /// exactly the "committed, forgot to rebuild" incident class), and only
    /// an EXACT match to HEAD -> Fresh.
    #[test]
    fn embedded_commit_is_stale_maps_ancestry_exit_codes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (base, side) = init_repo_with_diverged_commit(root);
        let head = run_git_stdout(root, &["rev-parse", "HEAD"])
            .expect("rev-parse HEAD")
            .trim()
            .to_string();

        // `base` is a strict ancestor of the fixture's final `trunk2` HEAD —
        // this previously asserted Fresh, which encoded the WR-01 bug (a
        // clean-tree binary built from `base` would have been misclassified
        // Fresh even though two commits landed on top of it since).
        assert_eq!(embedded_commit_is_stale(root, &base), Staleness::Stale);
        // The genuine Fresh case: an exact match to the current HEAD.
        assert_eq!(embedded_commit_is_stale(root, &head), Staleness::Fresh);
        assert_eq!(embedded_commit_is_stale(root, &side), Staleness::Stale);
        assert_eq!(embedded_commit_is_stale(root, ""), Staleness::Indeterminate);
        assert_eq!(
            embedded_commit_is_stale(root, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"),
            Staleness::Indeterminate
        );
    }

    /// WR-01 regression (17-06 gap closure): reproduces the verifier's exact
    /// live-reproduction narrative (17-VERIFICATION.md Gap 2 / Truth 10) — a
    /// LINEAR, clean-tree, two-commit fixture where the embedded commit
    /// legitimately IS an ancestor of the new HEAD, so `merge-base
    /// --is-ancestor` exits 0 and the mtime arm never runs on a clean tree.
    /// Before the WR-01 fix, this was misclassified Fresh; it must now be
    /// Stale, and `enforce_build_staleness` must hard-block a self-dogfood
    /// workspace in exactly this scenario — the Phase 16 "committed,
    /// forgot to rebuild" incident class this gate exists to catch.
    #[test]
    fn wr01_clean_tree_strict_ancestor_build_is_stale_and_hard_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);

        // First commit: a workspace Cargo.toml (both crate member paths) plus
        // one other tracked file.
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();
        std::fs::write(root.join("a.txt"), "one").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "workspace init"]);
        let embedded_commit = run_git_stdout(root, &["rev-parse", "HEAD"])
            .expect("rev-parse HEAD")
            .trim()
            .to_string();

        // Second commit on top: an unrelated NEW file — no modifications to
        // already-committed files, so the tree stays clean.
        std::fs::write(root.join("b.txt"), "two").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "unrelated follow-up"]);

        // Clean-tree property: this is what makes the mtime arm never run,
        // leaving the ancestry arm as the sole signal — exactly the gap the
        // WR-01 fix closes.
        let status = run_git_stdout(root, &["status", "--porcelain"]).unwrap();
        assert!(
            status.trim().is_empty(),
            "fixture must have a clean working tree"
        );

        assert_eq!(
            embedded_commit_is_stale(root, &embedded_commit),
            Staleness::Stale
        );
        assert_eq!(
            combined_staleness(root, &embedded_commit, false),
            Staleness::Stale
        );
        assert!(is_self_dogfood_workspace(root));

        let phase = 66;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;

        let err = enforce_build_staleness(root, &state, &embedded_commit, false).unwrap_err();
        assert!(
            err.to_string().contains("self-dogfood stale build blocked"),
            "{err}"
        );

        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("staleness block must record an event before returning the error");
        assert_eq!(last["event"], "self_dogfood_stale_blocked");
    }

    /// A binary built from a branch AHEAD of `project_root`'s HEAD is newer
    /// than the source it drives — the inverse of the "committed, forgot to
    /// rebuild" incident. `merge-base --is-ancestor <embedded> HEAD` exits 1
    /// for BOTH a descendant and a genuinely divergent/older commit, so the
    /// bare `Ok(Some(1)) => Stale` mapping hard-blocked a fresher build. Found
    /// live: this phase's own Validate stage was blocked by a binary built
    /// from `feature/phase-17` while the checkout sat on `develop`.
    #[test]
    fn ahead_build_from_descendant_commit_warns_instead_of_blocking() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);

        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();
        std::fs::write(root.join("a.txt"), "one").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "workspace init"]);
        let base_commit = run_git_stdout(root, &["rev-parse", "HEAD"])
            .expect("rev-parse HEAD")
            .trim()
            .to_string();

        // The build is made from the LATER commit...
        std::fs::write(root.join("b.txt"), "two").unwrap();
        git(&["add", "."]);
        git(&[
            "commit",
            "-q",
            "-m",
            "newer work the checkout does not have",
        ]);
        let embedded_commit = run_git_stdout(root, &["rev-parse", "HEAD"])
            .expect("rev-parse HEAD")
            .trim()
            .to_string();

        // ...while the checkout is moved BACK, leaving the embedded commit a
        // strict descendant of HEAD on a clean tree (so the mtime arm stays
        // out of it and ancestry is the sole signal).
        git(&["reset", "--hard", "-q", &base_commit]);
        let status = run_git_stdout(root, &["status", "--porcelain"]).unwrap();
        assert!(
            status.trim().is_empty(),
            "fixture must have a clean working tree"
        );

        assert_eq!(
            embedded_commit_is_stale(root, &embedded_commit),
            Staleness::Ahead,
            "a descendant embedded commit is newer than HEAD, not stale"
        );
        assert_eq!(
            staleness_outcome(true, Staleness::Ahead),
            StalenessOutcome::Warn,
            "an ahead build must warn, never hard-block, even for self-dogfood"
        );

        let phase = 67;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Validate;
        assert!(
            enforce_build_staleness(root, &state, &embedded_commit, false).is_ok(),
            "ahead build must not block a self-dogfood workspace"
        );
    }

    /// The live dirty-tree arm must only consider files that can change the
    /// compiled binary. Found live: DevFlow's own `ChangelogAppend` hook
    /// dirtied `CHANGELOG.md` during the Validate->Ship transition, an
    /// unfiltered check read that as a stale build, and the self-dogfood
    /// gate hard-blocked Ship — the pipeline blocking itself on a markdown
    /// file it had just written. A modified `.rs` file must still flag
    /// Stale (when the build was clean), or the gate stops catching the
    /// real "committed, forgot to rebuild" case (CR-02, 17-11: rewritten
    /// against the dirty-flag rule — the fixture's guarantees are
    /// unchanged, only the timestamp mechanism is gone).
    #[test]
    fn dirty_flag_arm_ignores_non_build_files_but_still_flags_sources() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);

        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();
        std::fs::write(root.join("CHANGELOG.md"), "# Changelog\n").unwrap();
        std::fs::create_dir_all(root.join("crates/devflow-cli/src")).unwrap();
        std::fs::write(
            root.join("crates/devflow-cli/src/main.rs"),
            "fn main() {}\n",
        )
        .unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "workspace init"]);
        let embedded_commit = run_git_stdout(root, &["rev-parse", "HEAD"])
            .expect("rev-parse HEAD")
            .trim()
            .to_string();

        // This binary was built from a CLEAN tree (the CR-02 incident
        // scenario): `build_dirty` is false throughout.
        let build_dirty = false;

        // Only a doc is dirty — exactly the live Ship-block condition.
        std::fs::write(root.join("CHANGELOG.md"), "# Changelog\n\n## 1.4.26\n").unwrap();
        assert_eq!(
            run_git_stdout(root, &["ls-files", "-m"]).unwrap().trim(),
            "CHANGELOG.md",
            "fixture must have exactly one dirty tracked file"
        );
        assert_eq!(
            tree_has_modified_build_inputs(root),
            Some(false),
            "a dirty CHANGELOG.md cannot change the compiled binary"
        );
        assert_eq!(
            combined_staleness(root, &embedded_commit, build_dirty),
            Staleness::Fresh,
            "a doc-only dirty tree must not be Stale"
        );

        let phase = 68;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Ship;
        assert!(
            enforce_build_staleness(root, &state, &embedded_commit, build_dirty).is_ok(),
            "a doc-only dirty tree must not block Ship"
        );

        // Converse: a dirty source file, on a build that was clean, IS
        // stale — the CR-02 case this whole plan exists to fix.
        std::fs::write(
            root.join("crates/devflow-cli/src/main.rs"),
            "fn main() { /* edited after build */ }\n",
        )
        .unwrap();
        assert_eq!(
            tree_has_modified_build_inputs(root),
            Some(true),
            "a modified .rs file is genuine staleness input"
        );

        // WR-03: the same edit, STAGED, must read identically. `git ls-files -m`
        // compares worktree-vs-index and goes silent once the edit is staged,
        // which let a stale binary certify itself as Fresh.
        git(&["add", "crates/devflow-cli/src/main.rs"]);
        assert!(
            !run_git_stdout(root, &["ls-files", "-m"])
                .unwrap()
                .lines()
                .any(|line| line.ends_with(".rs")),
            "fixture precondition: `ls-files -m` is blind to the staged .rs edit"
        );
        assert_eq!(
            tree_has_modified_build_inputs(root),
            Some(true),
            "a STAGED source edit is just as much a staleness input as an unstaged one"
        );
        assert_eq!(
            combined_staleness(root, &embedded_commit, build_dirty),
            Staleness::Stale,
            "a staged, uncommitted source edit on a clean build is Stale"
        );
        git(&["reset", "-q"]);
        assert_eq!(
            combined_staleness(root, &embedded_commit, build_dirty),
            Staleness::Stale
        );
        assert!(
            enforce_build_staleness(root, &state, &embedded_commit, build_dirty).is_err(),
            "a stale source build must still hard-block a self-dogfood workspace"
        );
    }

    /// D-19 composite/OR: a clean tree whose embedded commit IS an ancestor
    /// (HEAD itself) is Fresh regardless of `build_dirty`; but once a
    /// TRACKED, build-affecting file is modified (dirty tree) on a build
    /// that was made from a CLEAN tree, the dirty-flag arm flips the
    /// composite result to Stale even though ancestry alone says Fresh —
    /// this is the CR-02 case itself. CR-02 (17-11): renamed and rewritten
    /// against the dirty-flag rule (no more timestamp/mtime comparison);
    /// the test's *intent* — a second signal can flip an ancestry-Fresh
    /// result to Stale — survives unchanged.
    #[test]
    fn combined_staleness_dirty_flag_arm_flags_modified_tree_when_build_was_clean() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        // 17-10: the dirty file must be a BUILD-AFFECTING one. This fixture
        // used `a.txt`, which encoded the over-broad mtime arm that hard-blocked
        // Ship on a dirty CHANGELOG.md. The test's intent — a second signal
        // flips an ancestry-Fresh result to Stale — is unchanged; only the
        // fixture is corrected to a file that can actually change the binary.
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "// one\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        let head = {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };

        assert_eq!(embedded_commit_is_stale(root, &head), Staleness::Fresh);
        assert_eq!(combined_staleness(root, &head, false), Staleness::Fresh);

        std::fs::write(root.join("src/lib.rs"), "// modified after build\n").unwrap();
        assert_eq!(combined_staleness(root, &head, false), Staleness::Stale);
    }

    /// The Indeterminate branch of the decision table (must_haves truth 5,
    /// 17-11): a build made from an ALREADY-dirty tree, run against a tree
    /// that STILL has modified build inputs, cannot tell "same dirt" from
    /// "more dirt" without a timestamp — so it must be Indeterminate, never
    /// Stale, even though ancestry alone says Fresh. Pitfall 4: Indeterminate
    /// must never hard-block, even for a self-dogfood workspace.
    #[test]
    fn combined_staleness_dirty_flag_arm_is_indeterminate_when_build_was_already_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "// one\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        let head = run_git_stdout(root, &["rev-parse", "HEAD"])
            .expect("rev-parse HEAD")
            .trim()
            .to_string();
        assert!(is_self_dogfood_workspace(root));

        // The tree is dirty NOW (a build-affecting file is modified) — but
        // the embedded build's own dirty flag says it was ALSO built from a
        // dirty tree. Ancestry alone says Fresh (embedded_commit == HEAD).
        std::fs::write(root.join("src/lib.rs"), "// modified\n").unwrap();
        assert_eq!(embedded_commit_is_stale(root, &head), Staleness::Fresh);
        assert_eq!(
            tree_has_modified_build_inputs(root),
            Some(true),
            "fixture must have a dirty, build-affecting tree"
        );

        let build_was_dirty = true;
        assert_eq!(
            combined_staleness(root, &head, build_was_dirty),
            Staleness::Indeterminate,
            "cannot distinguish \"same dirt\" from \"more dirt\" without a timestamp"
        );

        let phase = 71;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;
        assert!(
            enforce_build_staleness(root, &state, &head, build_was_dirty).is_ok(),
            "Indeterminate must never hard-block, even for a self-dogfood workspace (Pitfall 4)"
        );
    }

    /// D-18: a self-dogfood workspace (matching `members = [...]`) with a
    /// confirmed-Stale embedded commit is a HARD block — but never silent:
    /// notify fires (best-effort; no `DEVFLOW_GATE_NOTIFY_CMD` is set here so
    /// it's a no-op) and an event is recorded BEFORE the blocking error is
    /// returned.
    #[test]
    fn enforce_build_staleness_blocks_self_dogfood_and_records_event_before_erroring() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (_base, side) = init_repo_with_diverged_commit(root);
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "add workspace cargo toml"]);
        assert!(is_self_dogfood_workspace(root));

        let phase = 63;
        let mut state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());
        state.stage = Stage::Code;

        let err = enforce_build_staleness(root, &state, &side, false).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("self-dogfood stale build blocked"),
            "{message}"
        );
        assert!(
            message.contains(&root.display().to_string()),
            "the returned CliError (terminal-only) must still name the path: {message}"
        );

        let last = devflow_core::events::last_event_for_phase(root, phase)
            .expect("staleness block must record an event before returning the error");
        assert_eq!(last["event"], "self_dogfood_stale_blocked");
        // WR-02 (18-fix): the persisted event's reason must be a bare,
        // path-free label — the full path-bearing message is for
        // fire_gate_notify/the returned Err only, never events.jsonl.
        assert_eq!(last["reason"], "stale_build_blocked");
        assert_eq!(last["worktree"], false);
        let reason_str = last["reason"].as_str().unwrap();
        assert!(
            !reason_str.contains(&root.display().to_string()),
            "persisted reason must never carry the project root path: {reason_str}"
        );
    }

    /// D-18: an ordinary (non-self-dogfood) project with the same confirmed-
    /// Stale embedded commit only warns and proceeds — no event, no error.
    #[test]
    fn enforce_build_staleness_warns_for_ordinary_project_with_stale_commit() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let (_base, side) = init_repo_with_diverged_commit(root);
        assert!(!is_self_dogfood_workspace(root));

        let phase = 64;
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());

        let result = enforce_build_staleness(root, &state, &side, false);
        assert!(
            result.is_ok(),
            "an ordinary project's stale build must only warn, never block"
        );
        assert!(
            devflow_core::events::last_event_for_phase(root, phase).is_none(),
            "a warn-only path must not fire the self_dogfood_stale_blocked event"
        );
    }

    /// Pitfall 4 / D-18: an Indeterminate result (unknown embedded commit)
    /// never hard-blocks, even for a self-dogfood workspace.
    #[test]
    fn enforce_build_staleness_never_blocks_on_indeterminate() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .output()
                    .unwrap()
                    .status
                    .success(),
                "git {args:?} failed"
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/devflow-core\", \"crates/devflow-cli\"]\n",
        )
        .unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        assert!(is_self_dogfood_workspace(root));

        let phase = 65;
        let state = State::new(phase, AgentKind::Claude, Mode::Auto, root.to_path_buf());

        let result = enforce_build_staleness(
            root,
            &state,
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
            false,
        );
        assert!(
            result.is_ok(),
            "an Indeterminate result must never hard-block"
        );
    }
}
