//! Release-cut observation oracles (29a) — pure classification logic plus
//! thin, read-only I/O wrappers around `git`/`gh`/`curl`.
//!
//! Every result is [`Observation`], a three-valued enum. `Unreachable` is
//! never conflated with `Absent` (RD-8's "unreachable != absent" rule): a
//! network partition, an unauthenticated `gh`, or a tool that fails to run
//! must always surface as `Unreachable`, carrying the real tool's own
//! failure text — never silently reported as "not done yet." This mirrors
//! `git.rs`'s existing `SigningViability::Unknown { reason }` /
//! `AncestorStatus` convention; nothing in this module invents a new oracle
//! shape.
//!
//! This module performs **no writes** — every function here only reads
//! remote or local state and reports what it observed.

use crate::git::git_command;
use std::path::Path;
use std::process::Command;

/// Three-valued oracle result for every observation in this module.
///
/// Every arm carries a `String` — never a bare unit variant — so the CLI
/// layer always has something to print. Two-valued (boolean) results are
/// forbidden anywhere in this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    /// The observed state is confirmed present.
    Present { detail: String },
    /// The observed state is confirmed absent (not "unknown" — a real
    /// negative answer from a reachable oracle).
    Absent { detail: String },
    /// The oracle could not be reached, or its answer could not be
    /// classified. Never collapsed into [`Absent`](Self::Absent) — a
    /// network or tool failure is not evidence of absence.
    Unreachable { reason: String },
}

/// The six release-cut questions this phase answers, in the exact order the
/// release sequence itself runs: version bump -> changelog -> release PR ->
/// signed tag -> sync back -> publish.
///
/// Only [`SignedTagPresent`](Self::SignedTagPresent) is wired to a real
/// oracle by this plan; [`CratesPublished`](Self::CratesPublished) is wired
/// by Task 2. The remaining four are wired in `29-02-PLAN.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStep {
    VersionBumped,
    ChangelogWritten,
    ReleasePrMerged,
    SignedTagPresent,
    SyncMerged,
    CratesPublished,
}

impl ReleaseStep {
    /// All six steps, in release-sequence order.
    pub const ALL: [ReleaseStep; 6] = [
        ReleaseStep::VersionBumped,
        ReleaseStep::ChangelogWritten,
        ReleaseStep::ReleasePrMerged,
        ReleaseStep::SignedTagPresent,
        ReleaseStep::SyncMerged,
        ReleaseStep::CratesPublished,
    ];

    /// A short human label for the `release status` report.
    pub fn label(self) -> &'static str {
        match self {
            ReleaseStep::VersionBumped => "version bumped on develop",
            ReleaseStep::ChangelogWritten => "changelog written on develop",
            ReleaseStep::ReleasePrMerged => "release PR merged to main",
            ReleaseStep::SignedTagPresent => "signed tag on origin",
            ReleaseStep::SyncMerged => "main synced back into develop",
            ReleaseStep::CratesPublished => "crates published",
        }
    }
}

/// A parsed `git ls-remote --tags` result for one tag name: the tag's own
/// object sha, and (if present) the peeled `^{}` sha that proves the tag is
/// annotated rather than lightweight.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagRefs {
    pub object: Option<String>,
    pub peeled: Option<String>,
}

impl TagRefs {
    /// `true` when a peeled entry was found — proves the tag is annotated,
    /// not that it is signed.
    pub fn is_annotated(&self) -> bool {
        self.peeled.is_some()
    }

    /// `true` when the tag's own ref was found at all.
    pub fn exists(&self) -> bool {
        self.object.is_some()
    }
}

/// Whether an annotated tag object carries a verified signature, as
/// determined by `gh api .../git/tags/<sha> --jq .verification.verified`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagSignature {
    Present,
    Missing,
    /// The GitHub API call did not answer (spawn failure, non-zero exit,
    /// unparseable output). Must propagate to [`Observation::Unreachable`],
    /// never collapse to [`Missing`](Self::Missing).
    Undetermined {
        reason: String,
    },
}

/// Parse `git ls-remote --tags` stdout for a specific tag. Pure — no I/O, no
/// `Command`, fully unit-testable. Tab-separated `<sha>\t<ref>` lines;
/// matches `refs/tags/<tag>` for the object sha and `refs/tags/<tag>^{}` for
/// the peeled sha. Every other line is ignored.
pub fn classify_tag_refs(ls_remote_stdout: &str, tag: &str) -> TagRefs {
    let want_object = format!("refs/tags/{tag}");
    let want_peeled = format!("refs/tags/{tag}^{{}}");
    let mut refs = TagRefs::default();
    for line in ls_remote_stdout.lines() {
        let Some((sha, refname)) = line.split_once('\t') else {
            continue;
        };
        if refname == want_object {
            refs.object = Some(sha.to_string());
        } else if refname == want_peeled {
            refs.peeled = Some(sha.to_string());
        }
    }
    refs
}

/// Classify a parsed [`TagRefs`] plus its resolved [`TagSignature`] into an
/// [`Observation`]. Pure — no I/O. Absent when the ref does not exist;
/// Absent (naming "lightweight") when it exists but is not annotated; then
/// delegates to the signature arm.
pub fn classify_signed_tag(refs: &TagRefs, signature: TagSignature) -> Observation {
    if !refs.exists() {
        return Observation::Absent {
            detail: "tag not found on origin".into(),
        };
    }
    if !refs.is_annotated() {
        return Observation::Absent {
            detail: "tag exists but is lightweight, not an annotated signed tag".into(),
        };
    }
    match signature {
        TagSignature::Present => Observation::Present {
            detail: "tag is annotated and signed".into(),
        },
        TagSignature::Missing => Observation::Absent {
            detail: "tag is annotated but carries no signature (unsigned)".into(),
        },
        TagSignature::Undetermined { reason } => Observation::Unreachable { reason },
    }
}

/// I/O wrapper: is `v<version>` present, annotated, and signed on `origin`?
/// Runs `git ls-remote --tags origin` through [`git_command`] (never a
/// direct, unscrubbed git invocation), passing the tag refspec as an
/// argument via `Command::args` — never string-interpolated into a shell
/// (T-29-01).
pub fn signed_tag_on_remote(project_root: &Path, version: &str) -> Observation {
    let tag = format!("v{version}");
    let pattern = format!("refs/tags/{tag}*");
    let output = git_command(project_root)
        .args(["ls-remote", "--tags", "origin", &pattern])
        .output();
    let output = match output {
        Ok(out) if out.status.success() => out,
        Ok(out) => {
            return Observation::Unreachable {
                reason: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            };
        }
        Err(err) => {
            return Observation::Unreachable {
                reason: err.to_string(),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let refs = classify_tag_refs(&stdout, &tag);
    // The tag OBJECT's own sha (the unpeeled `refs.object` entry) is what
    // `gh api .../git/tags/<sha>` expects — the peeled `^{}` entry
    // dereferences to the *commit* the tag object points to, a different
    // object with no `.verification` field of its own.
    let signature = if refs.is_annotated() {
        match &refs.object {
            Some(sha) => tag_signature_via_gh(project_root, sha),
            None => TagSignature::Missing,
        }
    } else {
        // Not annotated (or absent) — `classify_signed_tag` short-circuits
        // before consulting `signature` in that case, so this value is
        // never actually read; `Missing` is the harmless placeholder.
        TagSignature::Missing
    };
    classify_signed_tag(&refs, signature)
}

/// `gh api repos/{owner}/{repo}/git/tags/<sha> --jq .verification.verified`,
/// pinned to `project_root` so `gh`'s `{owner}`/`{repo}` placeholders
/// resolve to the project under observation rather than to whatever
/// repository the ambient shell happens to be in (T-29-10). `gh`
/// substitutes those placeholders from the repo context itself; the remote
/// URL is never parsed here.
///
/// The reason string on [`TagSignature::Undetermined`] describes the
/// failure class and may quote `gh`'s exit status, but never includes `gh`'s
/// raw stdout or stderr (T-17-13, inherited from `preflight.rs`, not
/// re-decided here).
fn tag_signature_via_gh(project_root: &Path, tag_object_sha: &str) -> TagSignature {
    let output = Command::new("gh")
        .current_dir(project_root)
        .args([
            "api",
            &format!("repos/{{owner}}/{{repo}}/git/tags/{tag_object_sha}"),
            "--jq",
            ".verification.verified",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => match String::from_utf8_lossy(&out.stdout).trim() {
            "true" => TagSignature::Present,
            "false" => TagSignature::Missing,
            _ => TagSignature::Undetermined {
                reason: "gh api returned an unexpected verification value".into(),
            },
        },
        Ok(out) => TagSignature::Undetermined {
            reason: format!("gh api exited with status {}", out.status),
        },
        Err(err) => TagSignature::Undetermined {
            reason: format!("failed to spawn gh: {}", err.kind()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotated_refs() -> TagRefs {
        TagRefs {
            object: Some("sha-object".into()),
            peeled: Some("sha-peeled".into()),
        }
    }

    fn lightweight_refs() -> TagRefs {
        TagRefs {
            object: Some("sha-object".into()),
            peeled: None,
        }
    }

    fn absent_refs() -> TagRefs {
        TagRefs::default()
    }

    #[test]
    fn classify_tag_refs_both_lines_yields_object_and_peeled() {
        let stdout = "abc123\trefs/tags/v2.1.0\ndef456\trefs/tags/v2.1.0^{}\n";
        let refs = classify_tag_refs(stdout, "v2.1.0");
        assert_eq!(refs.object.as_deref(), Some("abc123"));
        assert_eq!(refs.peeled.as_deref(), Some("def456"));
        assert!(refs.is_annotated());
        assert!(refs.exists());
    }

    #[test]
    fn classify_tag_refs_unpeeled_only_is_lightweight() {
        let stdout = "abc123\trefs/tags/v2.1.0\n";
        let refs = classify_tag_refs(stdout, "v2.1.0");
        assert_eq!(refs.object.as_deref(), Some("abc123"));
        assert_eq!(refs.peeled, None);
        assert!(!refs.is_annotated());
        assert!(refs.exists());
    }

    #[test]
    fn classify_tag_refs_empty_output_is_absent() {
        let refs = classify_tag_refs("", "v2.1.0");
        assert_eq!(refs.object, None);
        assert_eq!(refs.peeled, None);
        assert!(!refs.exists());
        assert!(!refs.is_annotated());
    }

    #[test]
    fn classify_tag_refs_ignores_unrelated_lines() {
        let stdout = "abc123\trefs/tags/v9.9.9\ndef456\trefs/heads/develop\n";
        let refs = classify_tag_refs(stdout, "v2.1.0");
        assert!(!refs.exists());
    }

    #[test]
    fn classify_signed_tag_absent_refs_is_absent() {
        let obs = classify_signed_tag(&absent_refs(), TagSignature::Missing);
        match obs {
            Observation::Absent { detail } => assert!(
                detail.to_lowercase().contains("tag"),
                "expected the detail to name the tag, got: {detail}"
            ),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn classify_signed_tag_lightweight_is_absent_naming_lightweight() {
        let obs = classify_signed_tag(&lightweight_refs(), TagSignature::Missing);
        match obs {
            Observation::Absent { detail } => assert!(
                detail.to_lowercase().contains("lightweight"),
                "expected the detail to state the tag is lightweight, got: {detail}"
            ),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn classify_signed_tag_annotated_present_is_present() {
        let obs = classify_signed_tag(&annotated_refs(), TagSignature::Present);
        assert!(matches!(obs, Observation::Present { .. }));
    }

    #[test]
    fn classify_signed_tag_annotated_missing_is_absent_unsigned() {
        let obs = classify_signed_tag(&annotated_refs(), TagSignature::Missing);
        match obs {
            Observation::Absent { detail } => assert!(
                detail.to_lowercase().contains("signature")
                    || detail.to_lowercase().contains("unsigned"),
                "expected the detail to state the tag is annotated but unsigned, got: {detail}"
            ),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn classify_signed_tag_annotated_undetermined_is_unreachable_with_reason_carried() {
        let obs = classify_signed_tag(
            &annotated_refs(),
            TagSignature::Undetermined {
                reason: "gh api exited with status 1".into(),
            },
        );
        match obs {
            Observation::Unreachable { reason } => {
                assert_eq!(reason, "gh api exited with status 1");
            }
            other => panic!("expected Unreachable, got {other:?}"),
        }
    }

    #[test]
    fn unreachable_is_never_absent() {
        // Constructing an Unreachable never produces something PartialEq to
        // an Absent with the same text — the two variants must never
        // compare equal even with identical strings, proving the type
        // system (not convention) keeps them apart.
        let unreachable = Observation::Unreachable {
            reason: "network unreachable".into(),
        };
        let absent = Observation::Absent {
            detail: "network unreachable".into(),
        };
        assert_ne!(unreachable, absent);
    }
}
