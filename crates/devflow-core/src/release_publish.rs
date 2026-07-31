//! Unit 29c — the commit point: the signed release tag, then the two
//! crates.io publishes, in the order [`crate::git::publish_order`] computes.
//!
//! Two irreversible operations. This module runs the exact commands
//! CONTRIBUTING.md and `git.rs`'s own `publish_order` already document and
//! reports back the real tool's own exit code and stderr — it predicts
//! neither whether signing will succeed nor whether a crate is already live;
//! it observes, attempts, and reports (D-10, RD-2's third layer). It writes
//! no progress record of its own: every step here is independently
//! re-runnable, because [`plan_local_tag`] and the per-crate crates.io
//! observation are consulted fresh on every call.
//!
//! [`create_and_push_tag`] is wired into
//! `release_execute::action_for(ReleaseStep::SignedTagPresent)`;
//! [`publish_all`] is wired into
//! `release_execute::action_for(ReleaseStep::CratesPublished)`.

use crate::git;
use crate::release_observe::{Observation, classify_http_status, crate_version_http_status};
use std::path::Path;
use std::time::Duration;

// ---------------------------------------------------------------------------
// The signed tag
// ---------------------------------------------------------------------------

/// What to do about the local tag namespace before running any tag command.
///
/// `hooks.rs`'s `VersionBump` hook (`crates/devflow-core/src/hooks.rs:278-337`)
/// already creates a local, lightweight, unsigned `v<version>` tag on
/// **every** ordinary phase Ship, unconditionally — so on any machine where a
/// phase has ever shipped, the name this release wants is already taken. The
/// tag verb refuses to overwrite an existing name without `-f`, so this is
/// the common case, not a rare edge case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagPlan {
    /// No local tag of this name exists — safe to create outright.
    CreateNew,
    /// A tag of this name exists at the target commit but is lightweight or
    /// annotated-without-a-signature — the hook's own artifact. Forcing over
    /// it is correct: it is not a release tag, and the release needs a
    /// signed one at the same commit. `existing` is the tag's current ref
    /// sha (harmless — never key material or a path).
    ReplaceLocal { existing: String },
    /// A tag of this name exists at the target commit, is annotated, and
    /// already carries a signature block — never force over it. Re-creating
    /// it could replace a possibly-correct signature with whatever key is
    /// ambient on this machine.
    LeaveAlone { existing: String },
    /// A tag of this name exists at a **different** commit. Forcing here
    /// would silently retarget a release name — the one outcome worse than
    /// stopping — so this refuses instead. `reason` names both commits.
    Refuse { reason: String },
}

/// Observe the local tag namespace for `tag` and decide what — if anything —
/// must happen to it before `tag` can be (re-)created at `target_commit`.
///
/// Determines annotated-versus-lightweight from the object type of the ref,
/// and signature presence by inspecting the tag object's own body for a
/// signature block — never by cryptographic verification. Verification needs
/// the signer's public key resolvable in whatever environment this runs in,
/// and its absence is a local tooling gap, not evidence about the tag;
/// presence of a signature block is answerable everywhere, and whose
/// signature it is remains `scripts/hooks/pre-push`'s question — it already
/// answers that by fingerprint comparison, and this function does not
/// duplicate that judgment.
pub fn plan_local_tag(project_root: &Path, tag: &str, target_commit: &str) -> TagPlan {
    let tag_ref = format!("refs/tags/{tag}");
    let ref_sha = match git::git_command(project_root)
        .args(["rev-parse", "--verify", "--quiet", &tag_ref])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => return TagPlan::CreateNew,
    };
    if ref_sha.is_empty() {
        return TagPlan::CreateNew;
    }

    let obj_type = match git::git_command(project_root)
        .args(["cat-file", "-t", &ref_sha])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => {
            return TagPlan::Refuse {
                reason: format!(
                    "could not determine the object type of the existing local tag {tag}"
                ),
            };
        }
    };
    let is_annotated = obj_type == "tag";

    let commit_sha = match git::git_command(project_root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{tag}^{{commit}}"),
        ])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => {
            return TagPlan::Refuse {
                reason: format!(
                    "could not resolve the commit the existing local tag {tag} points to"
                ),
            };
        }
    };

    if commit_sha != target_commit {
        return TagPlan::Refuse {
            reason: format!(
                "local tag {tag} already exists at commit {commit_sha}, but the release \
                 target is {target_commit} — refusing to retarget a release name"
            ),
        };
    }

    if !is_annotated {
        return TagPlan::ReplaceLocal { existing: ref_sha };
    }

    let body = match git::git_command(project_root)
        .args(["cat-file", "-p", &ref_sha])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        _ => {
            return TagPlan::Refuse {
                reason: format!("could not read the body of the existing local tag {tag}"),
            };
        }
    };
    let has_signature_block =
        body.contains("BEGIN SSH SIGNATURE") || body.contains("BEGIN PGP SIGNATURE");
    if has_signature_block {
        TagPlan::LeaveAlone { existing: ref_sha }
    } else {
        TagPlan::ReplaceLocal { existing: ref_sha }
    }
}

/// Pure argument-vector builder for the documented tag command
/// (CONTRIBUTING.md § "Cutting a Release" step 5): reproduces
///
/// ```text
/// git -c user.signingkey="$(git config --get devflow.releaseSigningKey)" \
///     tag -s vX.Y.Z <commit> -m "vX.Y.Z"
/// ```
///
/// exactly, in order — the scoped config override (only when `release_key`
/// is `Some`; a `None` key omits the override entirely rather than passing
/// an empty value, matching what the documented shell form degrades to when
/// `devflow.releaseSigningKey` is unset), the tag verb, the sign flag, the
/// force flag (only when `force` is set, inserted right after `-s`, matching
/// `scripts/hooks/pre-push`'s own remediation text), the tag name, the
/// target commit, and the message flag with the tag name as the message.
/// Returns owned strings so the built command can be asserted in tests
/// without executing anything.
pub fn tag_argv(release_key: Option<&str>, tag: &str, commit: &str, force: bool) -> Vec<String> {
    let mut argv = Vec::new();
    if let Some(key) = release_key {
        argv.push("-c".to_string());
        argv.push(format!("user.signingkey={key}"));
    }
    argv.push("tag".to_string());
    argv.push("-s".to_string());
    if force {
        argv.push("-f".to_string());
    }
    argv.push(tag.to_string());
    argv.push(commit.to_string());
    argv.push("-m".to_string());
    argv.push(tag.to_string());
    argv
}

/// Run the documented tag command for real. No pre-check predicts whether
/// signing will succeed (D-10, RD-5) — `git.rs`'s existing pre-flight
/// signing helper is reference material only and is never called from this
/// module. The error, when there is one, is git's own stderr, trimmed but
/// otherwise verbatim: that is where the pre-push hook's refusal and its
/// exact re-sign remediation command live, and suppressing it would leave
/// the operator with no path forward.
fn run_tag_command(
    project_root: &Path,
    release_key: Option<&str>,
    tag: &str,
    commit: &str,
    force: bool,
) -> Result<(), String> {
    let output = git::git_command(project_root)
        .args(tag_argv(release_key, tag, commit, force))
        .output()
        .map_err(|err| format!("failed to spawn git tag: {}", err.kind()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git tag exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Create (or leave alone) the signed release tag at `target_commit`, then
/// push it to `origin`. Reads `devflow.releaseSigningKey` from git config —
/// its value and any filesystem path derived from it are never included in
/// any `Ok`/`Err` string this function returns; if this module ever needs to
/// report anything about the key, it must be a public fingerprint, matching
/// the discipline `git.rs` already applies to `SigningViability`.
///
/// Acts on [`plan_local_tag`]'s answer: `CreateNew` and `ReplaceLocal` both
/// run the tag command (the latter with the force flag — forcing is only
/// ever reachable from the branch where a lightweight or unsigned duplicate
/// at the target commit was actually observed); `LeaveAlone` runs no tag
/// command at all; `Refuse` returns its reason without running anything.
pub fn create_and_push_tag(
    project_root: &Path,
    version: &str,
    target_commit: &str,
) -> Result<String, String> {
    let tag = format!("v{version}");
    let release_key = git::git_config(project_root, "devflow.releaseSigningKey");

    match plan_local_tag(project_root, &tag, target_commit) {
        TagPlan::Refuse { reason } => return Err(reason),
        TagPlan::CreateNew => {
            run_tag_command(
                project_root,
                release_key.as_deref(),
                &tag,
                target_commit,
                false,
            )?;
        }
        TagPlan::ReplaceLocal { .. } => {
            run_tag_command(
                project_root,
                release_key.as_deref(),
                &tag,
                target_commit,
                true,
            )?;
        }
        TagPlan::LeaveAlone { .. } => {
            // Already correct and signed at the target commit — never
            // re-create it (see plan_local_tag's doc comment). Only the
            // push, below, remains.
        }
    }

    let push = git::git_command(project_root)
        .args(["push", "origin", &tag])
        .output()
        .map_err(|err| format!("failed to spawn git push: {}", err.kind()))?;
    if push.status.success() {
        Ok(format!("tag {tag} created and pushed to origin"))
    } else {
        Err(format!(
            "git push origin {tag} exited with status {}: {}",
            push.status,
            String::from_utf8_lossy(&push.stderr).trim()
        ))
    }
}

// ---------------------------------------------------------------------------
// The publishes
// ---------------------------------------------------------------------------

/// The topologically-sorted crates.io publish order for this workspace,
/// exactly as [`crate::git::publish_order`] returns it — never re-sorted,
/// never filtered, and no crate name is ever written as a literal anywhere
/// in this module. An empty result means the workspace `Cargo.toml` could
/// not be read or declares no local-path members, which is a failure to
/// determine the plan, not "nothing to publish."
pub fn publish_plan(project_root: &Path) -> Result<Vec<String>, String> {
    let order = git::publish_order(project_root);
    if order.is_empty() {
        Err(
            "publish_order returned no crates — the workspace Cargo.toml could not be read \
             or declares no local-path members"
                .to_string(),
        )
    } else {
        Ok(order)
    }
}

/// Bounded re-observation window after a successful publish: a handful of
/// attempts over roughly thirty seconds, not an open-ended loop. This
/// answers a different oracle than `cargo publish`'s own internal wait (the
/// database-backed crates.io JSON API, never the CDN-cached sparse index —
/// see 29-RESEARCH.md's Pitfall 2), so it needs no relationship to cargo's
/// own timeout.
const REOBSERVE_ATTEMPTS: u32 = 6;
const REOBSERVE_INTERVAL: Duration = Duration::from_secs(5);

/// `curl` the crates.io `/api/v1` JSON endpoint for `name`/`version` — the
/// same oracle [`crate::release_observe::crates_published`] already uses per
/// crate — and classify the result with [`classify_http_status`]. Never a
/// second implementation of either the HTTP call or the classification.
fn observe_crate_published(project_root: &Path, name: &str, version: &str) -> Observation {
    match crate_version_http_status(project_root, name, version) {
        Ok(code) => classify_http_status(&code),
        Err(reason) => Observation::Unreachable { reason },
    }
}

/// `cargo publish -p <member>`, via [`crate::git::hermetic_command`] rather
/// than a bare `Command::new("cargo")` — `cargo`'s own git-invoking build
/// scripts are the documented motivating case for that scrub (Phase 27,
/// T-29-05), and this is exactly the kind of call that would reopen it.
fn run_cargo_publish(project_root: &Path, member: &str) -> Result<(), String> {
    let output = git::hermetic_command("cargo", project_root)
        .args(["publish", "-p", member])
        .output()
        .map_err(|err| format!("failed to spawn cargo publish: {}", err.kind()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "cargo publish -p {member} exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Publish every member [`publish_plan`] returns, in that exact order,
/// observing each crate's publish state before and after publishing it.
/// Wires the real crates.io observer and the real `cargo publish` invocation
/// into [`publish_members`] — the injectable core this module's tests
/// exercise with no network dependency at all.
pub fn publish_all(project_root: &Path, version: &str) -> Result<Vec<String>, String> {
    let members = publish_plan(project_root)?;
    publish_members(
        project_root,
        version,
        &members,
        REOBSERVE_ATTEMPTS,
        REOBSERVE_INTERVAL,
        observe_crate_published,
        run_cargo_publish,
    )
}

/// The injectable core of [`publish_all`]: `observe` and `publish` are
/// function parameters, so every behavior here — skip-what's-already-done,
/// stop-on-unreachable, stop-on-failed-publish-with-no-rollback, the bounded
/// re-observation window, and, most importantly, that `members` is consumed
/// in exactly the order given with no re-sorting — is unit-testable against
/// an injected sequence with zero network dependency.
///
/// For each member, in order:
/// 1. Observe its publish state at `version`. `Present` skips it (recorded,
///    never attempted); `Unreachable` stops the whole run before any publish
///    is attempted — an unreachable registry must never be read as "not
///    published," because acting on that reading is how a completed
///    irreversible step gets redone; `Absent` proceeds to step 2.
/// 2. Publish it. A failure stops the run immediately, carrying the real
///    tool's own stderr; no later member is attempted and no earlier
///    publish is undone (RD-6: fail-fast, no automatic rollback — safe
///    because every step is independently re-runnable from wherever the
///    world actually is).
/// 3. Re-observe with a bounded wait before moving to the next member.
///    Exhausting the bound returns an error naming the member and the bound
///    rather than proceeding — the next member resolves against the
///    registry and will fail to build if this one is not yet live. cargo's
///    own duplicate-version rejection remains the authoritative final gate;
///    step 1's pre-check is a skip-what-is-done optimization only, never the
///    sole thing standing between the run and a duplicate publish attempt.
fn publish_members(
    project_root: &Path,
    version: &str,
    members: &[String],
    reobserve_attempts: u32,
    reobserve_interval: Duration,
    observe: impl Fn(&Path, &str, &str) -> Observation,
    publish: impl Fn(&Path, &str) -> Result<(), String>,
) -> Result<Vec<String>, String> {
    let mut report = Vec::new();

    for member in members {
        match observe(project_root, member, version) {
            Observation::Present { .. } => {
                report.push(format!("{member}: already published at {version}, skipped"));
                continue;
            }
            Observation::Unreachable { reason } => {
                return Err(format!(
                    "could not observe {member}'s publish state before publishing — refusing \
                     to act on an unreachable registry: {reason}"
                ));
            }
            Observation::Absent { .. } => {}
        }

        publish(project_root, member)?;

        let mut confirmed = false;
        for attempt in 0..reobserve_attempts {
            match observe(project_root, member, version) {
                Observation::Present { .. } => {
                    confirmed = true;
                    break;
                }
                Observation::Unreachable { reason } => {
                    return Err(format!(
                        "{member} published, but could not be re-observed afterward: {reason}"
                    ));
                }
                Observation::Absent { .. } => {
                    if attempt + 1 < reobserve_attempts {
                        std::thread::sleep(reobserve_interval);
                    }
                }
            }
        }
        if !confirmed {
            return Err(format!(
                "{member} did not become available on crates.io within {reobserve_attempts} \
                 re-observation attempts after publishing — refusing to publish the next \
                 member against a registry that has not yet caught up"
            ));
        }

        report.push(format!("{member}: published at {version}"));
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::GitFlow;
    use std::cell::{Cell, RefCell};
    use tempfile::TempDir;

    fn git_at(root: &Path, args: &[&str]) {
        let output = git::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} in {root:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn git_output_at(root: &Path, args: &[&str]) -> String {
        let output = git::git_command(root)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} in {root:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// A minimal single-commit repo with no tags and no remote — the fixture
    /// every `plan_local_tag`/`create_and_push_tag` test builds on.
    fn init_tag_test_repo() -> (TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git_at(root, &["init", "-q"]);
        git_at(root, &["config", "user.email", "test@example.com"]);
        git_at(root, &["config", "user.name", "Test"]);
        git_at(root, &["config", "commit.gpgsign", "false"]);
        git_at(root, &["config", "tag.gpgsign", "false"]);
        git_at(root, &["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "base\n").unwrap();
        git_at(root, &["add", "README.md"]);
        git_at(root, &["commit", "-q", "-m", "base"]);
        let commit = git_output_at(root, &["rev-parse", "HEAD"]);
        (dir, commit)
    }

    // -- tag_command_form ----------------------------------------------------

    #[test]
    fn tag_command_form_with_explicit_key_matches_contributing_md() {
        let argv = tag_argv(Some("~/.ssh/release.pub"), "v1.2.3", "abc123", false);
        assert_eq!(
            argv.join(" "),
            "-c user.signingkey=~/.ssh/release.pub tag -s v1.2.3 abc123 -m v1.2.3"
        );
    }

    #[test]
    fn tag_command_form_without_key_omits_override() {
        let argv = tag_argv(None, "v1.2.3", "abc123", false);
        assert!(
            !argv.iter().any(|a| a == "-c"),
            "expected no -c override when release_key is None: {argv:?}"
        );
        assert_eq!(
            argv,
            vec!["tag", "-s", "v1.2.3", "abc123", "-m", "v1.2.3"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn tag_command_form_force_inserts_flag_only() {
        let without_force = tag_argv(Some("key"), "v1.2.3", "abc123", false);
        let with_force = tag_argv(Some("key"), "v1.2.3", "abc123", true);
        assert!(with_force.contains(&"-f".to_string()));
        assert!(!without_force.contains(&"-f".to_string()));
        let with_force_minus_flag: Vec<String> = with_force
            .iter()
            .filter(|a| a.as_str() != "-f")
            .cloned()
            .collect();
        assert_eq!(
            with_force_minus_flag, without_force,
            "expected force to change nothing except inserting -f"
        );
    }

    // -- tag_namespace_collision ----------------------------------------------

    #[test]
    fn tag_namespace_collision_no_existing_tag_yields_create_new() {
        let (repo, commit) = init_tag_test_repo();
        assert_eq!(
            plan_local_tag(repo.path(), "v1.2.3", &commit),
            TagPlan::CreateNew
        );
    }

    #[test]
    fn tag_namespace_collision_lightweight_at_target_commit_yields_replace_local() {
        let (repo, commit) = init_tag_test_repo();
        // Exactly as `hooks_after_ship`'s VersionBump hook creates it.
        GitFlow::new(repo.path())
            .tag("v1.2.3")
            .expect("create lightweight tag");
        let plan = plan_local_tag(repo.path(), "v1.2.3", &commit);
        assert!(
            matches!(plan, TagPlan::ReplaceLocal { .. }),
            "expected ReplaceLocal, got {plan:?}"
        );
    }

    #[test]
    fn tag_namespace_collision_annotated_without_a_signature_block_at_target_commit_yields_replace_local()
     {
        let (repo, commit) = init_tag_test_repo();
        git_at(
            repo.path(),
            &["tag", "-a", "v1.2.3", &commit, "-m", "v1.2.3"],
        );
        let plan = plan_local_tag(repo.path(), "v1.2.3", &commit);
        assert!(
            matches!(plan, TagPlan::ReplaceLocal { .. }),
            "expected ReplaceLocal for an annotated-but-unsigned tag, got {plan:?}"
        );
    }

    #[test]
    fn tag_namespace_collision_annotated_with_a_signature_block_at_target_commit_yields_leave_alone()
     {
        let (repo, commit) = init_tag_test_repo();
        // plan_local_tag checks for signature-block PRESENCE, not
        // cryptographic validity (see its own doc comment) — a synthetic
        // marker embedded in the annotation message is a legitimate fixture
        // for this classification.
        git_at(
            repo.path(),
            &[
                "tag",
                "-a",
                "v1.2.3",
                &commit,
                "-m",
                "v1.2.3\n-----BEGIN SSH SIGNATURE-----\nfake\n-----END SSH SIGNATURE-----",
            ],
        );
        let plan = plan_local_tag(repo.path(), "v1.2.3", &commit);
        assert!(
            matches!(plan, TagPlan::LeaveAlone { .. }),
            "expected LeaveAlone for an annotated tag carrying a signature block, got {plan:?}"
        );
    }

    #[test]
    fn tag_namespace_collision_different_commit_yields_refuse_naming_both_commits() {
        let (repo, first_commit) = init_tag_test_repo();
        std::fs::write(repo.path().join("other.txt"), "x\n").unwrap();
        git_at(repo.path(), &["add", "other.txt"]);
        git_at(repo.path(), &["commit", "-q", "-m", "second"]);
        let second_commit = git_output_at(repo.path(), &["rev-parse", "HEAD"]);
        git_at(repo.path(), &["tag", "v1.2.3", &first_commit]);

        let plan = plan_local_tag(repo.path(), "v1.2.3", &second_commit);
        match plan {
            TagPlan::Refuse { reason } => {
                assert!(
                    reason.contains(&first_commit),
                    "expected the reason to name the existing commit, got: {reason}"
                );
                assert!(
                    reason.contains(&second_commit),
                    "expected the reason to name the release target commit, got: {reason}"
                );
            }
            other => panic!("expected Refuse, got {other:?}"),
        }
    }

    // -- create_and_push_tag: real git failures surface verbatim --------------

    #[test]
    fn tag_command_failure_surfaces_gits_stderr_verbatim() {
        let (repo, _commit) = init_tag_test_repo();
        // git resolves the target object before it ever attempts to sign,
        // so this fails deterministically regardless of the host's signing
        // configuration — no gpg/ssh-agent dependency in this test.
        let result = run_tag_command(repo.path(), None, "v1.2.3", "not-a-real-object", false);
        let err = result.expect_err("expected the tag command to fail against a bad object");
        assert!(
            err.contains("not-a-real-object"),
            "expected git's own error naming the bad object, got: {err}"
        );
    }

    #[test]
    fn tag_push_failure_surfaces_gits_stderr_verbatim() {
        let (repo, commit) = init_tag_test_repo();
        // An annotated tag carrying a signature-block marker at the target
        // commit classifies as LeaveAlone, so create_and_push_tag runs no
        // tag command at all (no gpg/ssh-agent dependency) and goes
        // straight to the push — which fails deterministically because no
        // `origin` remote is configured.
        git_at(
            repo.path(),
            &[
                "tag",
                "-a",
                "v1.2.3",
                &commit,
                "-m",
                "v1.2.3\n-----BEGIN SSH SIGNATURE-----\nfake\n-----END SSH SIGNATURE-----",
            ],
        );
        let result = create_and_push_tag(repo.path(), "1.2.3", &commit);
        let err = result.expect_err("expected the push to fail with no origin configured");
        assert!(
            !err.is_empty(),
            "expected a real, non-empty push failure reason"
        );
    }

    #[test]
    fn create_and_push_tag_refuses_before_running_any_command_on_a_different_commit_collision() {
        let (repo, first_commit) = init_tag_test_repo();
        std::fs::write(repo.path().join("other.txt"), "x\n").unwrap();
        git_at(repo.path(), &["add", "other.txt"]);
        git_at(repo.path(), &["commit", "-q", "-m", "second"]);
        let second_commit = git_output_at(repo.path(), &["rev-parse", "HEAD"]);
        git_at(repo.path(), &["tag", "v1.2.3", &first_commit]);

        let result = create_and_push_tag(repo.path(), "1.2.3", &second_commit);
        let err = result.expect_err("expected a refusal");
        assert!(err.contains(&first_commit) && err.contains(&second_commit));
    }

    // -- publish_plan ----------------------------------------------------------

    fn init_two_crate_workspace() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("crates/a")).unwrap();
        std::fs::create_dir_all(root.join("crates/b")).unwrap();
        std::fs::write(
            root.join("crates/a/Cargo.toml"),
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            root.join("crates/b/Cargo.toml"),
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\n\n[dependencies]\na = { path = \"../a\" }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn publish_plan_follows_git_publish_order_dependency_before_dependent() {
        let dir = init_two_crate_workspace();
        let plan = publish_plan(dir.path()).expect("publish_plan");
        assert_eq!(plan, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn publish_plan_on_a_directory_with_no_workspace_manifest_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            publish_plan(dir.path()).is_err(),
            "expected an error when no workspace Cargo.toml exists"
        );
    }

    // -- publish_order_respected: the consumer follows, never corrects --------

    #[test]
    fn publish_order_respected_follows_the_given_order_without_resorting() {
        let published_order = RefCell::new(Vec::new());
        let observe_calls = Cell::new(0u32);
        let members = vec!["zeta".to_string(), "alpha".to_string(), "mu".to_string()];

        let result = publish_members(
            Path::new("/nonexistent"),
            "1.2.3",
            &members,
            1,
            Duration::from_millis(0),
            |_, _, _| {
                let n = observe_calls.get();
                observe_calls.set(n + 1);
                if n.is_multiple_of(2) {
                    Observation::Absent {
                        detail: "not yet".into(),
                    }
                } else {
                    Observation::Present {
                        detail: "now published".into(),
                    }
                }
            },
            |_, member| {
                published_order.borrow_mut().push(member.to_string());
                Ok(())
            },
        );

        assert!(
            result.is_ok(),
            "expected the run to succeed, got {result:?}"
        );
        assert_eq!(
            published_order.into_inner(),
            vec!["zeta".to_string(), "alpha".to_string(), "mu".to_string()],
            "expected the members to be published in exactly the given order"
        );
    }

    #[test]
    fn publish_order_respected_a_reversed_order_is_followed_not_corrected() {
        let published_order = RefCell::new(Vec::new());
        let observe_calls = Cell::new(0u32);
        let members = vec!["z".to_string(), "a".to_string()];

        let result = publish_members(
            Path::new("/nonexistent"),
            "1.2.3",
            &members,
            1,
            Duration::from_millis(0),
            |_, _, _| {
                let n = observe_calls.get();
                observe_calls.set(n + 1);
                if n.is_multiple_of(2) {
                    Observation::Absent { detail: "x".into() }
                } else {
                    Observation::Present { detail: "y".into() }
                }
            },
            |_, member| {
                published_order.borrow_mut().push(member.to_string());
                Ok(())
            },
        );

        assert!(
            result.is_ok(),
            "expected the run to succeed, got {result:?}"
        );
        assert_eq!(
            published_order.into_inner(),
            vec!["z".to_string(), "a".to_string()],
            "expected the reversed input order to be followed, not alphabetically corrected"
        );
    }

    // -- publish_members: skip / publish / unreachable / failure / bound ------

    #[test]
    fn publish_members_skips_a_crate_already_observed_published() {
        let members = vec!["only".to_string()];
        let publish_called = Cell::new(false);

        let result = publish_members(
            Path::new("/nonexistent"),
            "1.2.3",
            &members,
            1,
            Duration::from_millis(0),
            |_, _, _| Observation::Present {
                detail: "already there".into(),
            },
            |_, _| {
                publish_called.set(true);
                Ok(())
            },
        );

        let report = result.expect("expected success");
        assert!(
            !publish_called.get(),
            "expected an already-published crate to never be published"
        );
        assert_eq!(report.len(), 1);
        assert!(report[0].contains("already published"));
    }

    #[test]
    fn publish_members_publishes_a_crate_observed_not_published() {
        let members = vec!["only".to_string()];
        let publish_called = Cell::new(false);
        let observe_calls = Cell::new(0u32);

        let result = publish_members(
            Path::new("/nonexistent"),
            "1.2.3",
            &members,
            1,
            Duration::from_millis(0),
            |_, _, _| {
                let n = observe_calls.get();
                observe_calls.set(n + 1);
                if n == 0 {
                    Observation::Absent {
                        detail: "not yet".into(),
                    }
                } else {
                    Observation::Present {
                        detail: "now".into(),
                    }
                }
            },
            |_, _| {
                publish_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_ok(), "expected success, got {result:?}");
        assert!(publish_called.get(), "expected the crate to be published");
    }

    #[test]
    fn publish_members_stops_the_run_when_a_per_crate_observation_is_unreachable_and_publishes_nothing()
     {
        let members = vec!["a".to_string(), "b".to_string()];
        let publish_calls = Cell::new(0u32);

        let result = publish_members(
            Path::new("/nonexistent"),
            "1.2.3",
            &members,
            1,
            Duration::from_millis(0),
            |_, _, _| Observation::Unreachable {
                reason: "registry down".into(),
            },
            |_, _| {
                publish_calls.set(publish_calls.get() + 1);
                Ok(())
            },
        );

        let err = result.expect_err("expected the run to stop on an unreachable observation");
        assert!(err.contains("registry down"), "got: {err}");
        assert_eq!(publish_calls.get(), 0, "expected zero publish invocations");
    }

    #[test]
    fn publish_members_stops_on_a_failed_publish_and_carries_cargos_stderr_and_attempts_no_later_crate()
     {
        let members = vec!["a".to_string(), "b".to_string()];
        let publish_calls = RefCell::new(Vec::new());

        let result = publish_members(
            Path::new("/nonexistent"),
            "1.2.3",
            &members,
            1,
            Duration::from_millis(0),
            |_, _, _| Observation::Absent {
                detail: "not yet".into(),
            },
            |_, member| {
                publish_calls.borrow_mut().push(member.to_string());
                Err(
                    "cargo publish -p a exited with status 101: error: crates.io index refused \
                     the request"
                        .to_string(),
                )
            },
        );

        let err = result.expect_err("expected the failed publish to stop the run");
        assert!(
            err.contains("crates.io index refused the request"),
            "got: {err}"
        );
        assert_eq!(
            publish_calls.into_inner(),
            vec!["a".to_string()],
            "expected only the first member to be attempted"
        );
    }

    #[test]
    fn publish_members_bounded_reobservation_returns_an_error_naming_the_crate_and_bound_when_availability_never_appears()
     {
        let members = vec!["stuck".to_string()];

        let result = publish_members(
            Path::new("/nonexistent"),
            "1.2.3",
            &members,
            3,
            Duration::from_millis(0),
            |_, _, _| Observation::Absent {
                detail: "not yet".into(),
            },
            |_, _| Ok(()),
        );

        let err = result.expect_err("expected exhausting the re-observation bound to be an error");
        assert!(err.contains("stuck"), "expected the crate name, got: {err}");
        assert!(err.contains('3'), "expected the bound, got: {err}");
    }

    #[test]
    fn publish_members_observes_every_member_even_when_all_are_already_published() {
        let members = vec!["a".to_string(), "b".to_string()];
        let observed = RefCell::new(Vec::new());

        let result = publish_members(
            Path::new("/nonexistent"),
            "1.2.3",
            &members,
            1,
            Duration::from_millis(0),
            |_, name, _| {
                observed.borrow_mut().push(name.to_string());
                Observation::Present {
                    detail: "already published".into(),
                }
            },
            |_, _| panic!("must not publish an already-published member"),
        );

        assert!(result.is_ok(), "expected success, got {result:?}");
        assert_eq!(
            observed.into_inner(),
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
