//! The release-cut walker (29b/29c scaffolding): observe, act, re-observe,
//! stop — the design rule (RD-2) as executable control flow.
//!
//! [`cut`] walks [`ReleaseStep::ALL`] in order, observing each step via
//! [`crate::release_observe::observe`]. `Present` records the step done and
//! continues; `Unreachable` stops the walk and refuses immediately — an
//! oracle that could not be reached must never lead to an action, because
//! acting on it risks redoing an already-completed irreversible step;
//! `Absent` checks for an in-flight pull request before running the step's
//! own action (if this build carries one), then stops either way. This
//! build carries **no step actions** — [`action_for`] returns `None` for
//! every one of the six [`ReleaseStep`] variants. `29-05`, `29-06`, and
//! `29-07` each replace specific `None` arms with real actions; the match is
//! exhaustive over all six variants with no wildcard arm, so a new step
//! cannot be silently skipped.
//!
//! This module performs **no writes** to `.devflow/`, to `devflow.toml`, or
//! to any other DevFlow-owned file, and holds no state across invocations.
//! Re-running is re-observing.

use crate::release_observe::{Observation, ReleaseStep, observe};
use std::path::Path;
use std::process::Command;

/// The outcome of walking one [`ReleaseStep`].
///
/// Every arm carries a `String` (or, for [`NoActionInThisBuild`](Self::NoActionInThisBuild),
/// the unit identifier) — never a bare unit variant — so the CLI layer
/// always has something to print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    /// The step was already observed `Present` before this run touched it.
    AlreadyDone { detail: String },
    /// The step is `Absent` but a pull request for it is already open —
    /// under way, not a candidate for a duplicate.
    InFlight { detail: String },
    /// This run's own action attempt succeeded. Terminal on purpose: the
    /// action's real effect (e.g. GitHub's auto-merge) is asynchronous, and
    /// continuing past it would mean predicting that outcome, which RD-2's
    /// third layer forbids.
    Performed { detail: String },
    /// The walk stopped here — an unreachable oracle, an unanswerable
    /// in-flight check, or a failed action.
    Stopped { reason: String },
    /// The step is `Absent`, no pull request is in flight, and this build
    /// carries no action for it yet. `unit` names the unit that supplies it
    /// (`29b` or `29c`).
    NoActionInThisBuild { unit: &'static str },
}

impl StepOutcome {
    /// `true` for every variant except [`AlreadyDone`](Self::AlreadyDone) —
    /// the walk stops on all four terminal outcomes, and only `AlreadyDone`
    /// continues it.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, StepOutcome::AlreadyDone { .. })
    }
}

/// The full record of one `cut` walk: one outcome per step actually
/// reached, in [`ReleaseStep::ALL`] order, and whether the run was
/// authorized at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutReport {
    pub steps: Vec<(ReleaseStep, StepOutcome)>,
    pub authorized: bool,
}

impl CutReport {
    /// The first step whose outcome is not [`StepOutcome::AlreadyDone`] —
    /// where the walk actually stopped, if it stopped at all.
    pub fn stopped_at(&self) -> Option<(ReleaseStep, &StepOutcome)> {
        self.steps.iter().find_map(|(step, outcome)| match outcome {
            StepOutcome::AlreadyDone { .. } => None,
            other => Some((*step, other)),
        })
    }

    /// `true` only when every one of the six steps observed `AlreadyDone` —
    /// the only case in which the release-cut walk has nothing left to do.
    pub fn all_done(&self) -> bool {
        self.stopped_at().is_none()
    }
}

/// A step's action: given the project root and the version, attempt the
/// step's real-world effect. `Ok` carries a human detail; `Err` carries the
/// real failure text.
type StepAction = fn(&Path, &str) -> Result<String, String>;

/// The action this build carries for `step`, if any. Returns `None` for
/// every one of the six variants in this build — `29-05`, `29-06`, and
/// `29-07` each replace a specific arm. Exhaustive with no wildcard arm: a
/// seventh [`ReleaseStep`] variant fails to compile here rather than
/// silently falling through to `None`.
fn action_for(step: ReleaseStep) -> Option<StepAction> {
    match step {
        ReleaseStep::VersionBumped => None,
        ReleaseStep::ChangelogWritten => None,
        ReleaseStep::ReleasePrMerged => None,
        ReleaseStep::SignedTagPresent => None,
        ReleaseStep::SyncMerged => None,
        ReleaseStep::CratesPublished => None,
    }
}

/// The unit identifier that supplies `step`'s action — used only to make a
/// [`StepOutcome::NoActionInThisBuild`] report name which future unit closes
/// the gap. Steps 1, 2, 3, and 5 (the recoverable PR-backed actions) belong
/// to `29b`; steps 4 and 6 (the two irreversible commit-point operations)
/// belong to `29c`.
fn unit_for(step: ReleaseStep) -> &'static str {
    match step {
        ReleaseStep::VersionBumped => "29b",
        ReleaseStep::ChangelogWritten => "29b",
        ReleaseStep::ReleasePrMerged => "29b",
        ReleaseStep::SignedTagPresent => "29c",
        ReleaseStep::SyncMerged => "29b",
        ReleaseStep::CratesPublished => "29c",
    }
}

/// Whether an open pull request already exists for a pull-request-backed
/// step — three-valued like every other observation in this phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrPresence {
    Open { number: u64 },
    None,
    Unreachable { reason: String },
}

/// The head and base branch names for a pull-request-backed [`ReleaseStep`],
/// or `None` for [`ReleaseStep::SignedTagPresent`] and
/// [`ReleaseStep::CratesPublished`], which are not pull-request-backed. The
/// head branch names are deterministic functions of `version` so a re-run
/// finds the same branch.
pub fn pr_refs(step: ReleaseStep, version: &str) -> Option<(String, String)> {
    match step {
        ReleaseStep::VersionBumped | ReleaseStep::ChangelogWritten => {
            Some((format!("release/bump-v{version}"), "develop".to_string()))
        }
        ReleaseStep::ReleasePrMerged => Some(("develop".to_string(), "main".to_string())),
        ReleaseStep::SignedTagPresent => None,
        ReleaseStep::SyncMerged => Some((
            format!("sync/main-to-develop-v{version}"),
            "develop".to_string(),
        )),
        ReleaseStep::CratesPublished => None,
    }
}

/// `gh pr list --state open --head <head> --base <base> --json number --jq
/// '.[0].number'`, pinned to `project_root` (T-29-10). Empty stdout is
/// [`PrPresence::None`]; a parseable number is [`PrPresence::Open`]; a spawn
/// failure, non-zero exit, or unparseable stdout is
/// [`PrPresence::Unreachable`] with a failure-class reason that never embeds
/// `gh`'s raw output (T-29-03).
pub fn open_pr(project_root: &Path, head: &str, base: &str) -> PrPresence {
    let output = Command::new("gh")
        .current_dir(project_root)
        .args([
            "pr",
            "list",
            "--state",
            "open",
            "--head",
            head,
            "--base",
            base,
            "--json",
            "number",
            "--jq",
            ".[0].number",
        ])
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if text.is_empty() {
                PrPresence::None
            } else {
                match text.parse::<u64>() {
                    Ok(number) => PrPresence::Open { number },
                    Err(_) => PrPresence::Unreachable {
                        reason: "gh pr list returned an unparseable pull request number".into(),
                    },
                }
            }
        }
        Ok(out) => PrPresence::Unreachable {
            reason: format!("gh pr list exited with status {}", out.status),
        },
        Err(err) => PrPresence::Unreachable {
            reason: format!("failed to spawn gh pr list: {}", err.kind()),
        },
    }
}

/// The mandate-refusal reason, naming all three ways to grant a release-cut
/// mandate. Shared by [`cut_with`]'s unauthorized-refusal path so the CLI
/// layer and every test see byte-identical wording.
fn no_mandate_reason() -> String {
    "a release requires an explicit mandate — grant one via --yes-release, \
     `yes_release = true` in devflow.toml, or DEVFLOW_YES_RELEASE"
        .to_string()
}

/// Walk the six release-cut steps, observing each via
/// [`crate::release_observe::observe`]. See the module doc comment for the
/// walk's shape.
pub fn cut(project_root: &Path, version: &str, authorized: bool) -> CutReport {
    cut_with(project_root, version, authorized, |root, step, version| {
        observe(root, step, version)
    })
}

/// The walk's control flow, with the observation source injected — lets
/// tests exercise the walk against a sequence of [`Observation`]s with no
/// network. [`cut`] passes [`crate::release_observe::observe`].
fn cut_with(
    project_root: &Path,
    version: &str,
    authorized: bool,
    observe_fn: impl Fn(&Path, ReleaseStep, &str) -> Observation,
) -> CutReport {
    if !authorized {
        return CutReport {
            steps: vec![(
                ReleaseStep::ALL[0],
                StepOutcome::Stopped {
                    reason: no_mandate_reason(),
                },
            )],
            authorized: false,
        };
    }

    let mut steps = Vec::new();
    for step in ReleaseStep::ALL {
        match observe_fn(project_root, step, version) {
            Observation::Present { detail } => {
                steps.push((step, StepOutcome::AlreadyDone { detail }));
            }
            Observation::Unreachable { reason } => {
                steps.push((step, StepOutcome::Stopped { reason }));
                break;
            }
            Observation::Absent { .. } => {
                if let Some((head, base)) = pr_refs(step, version) {
                    match open_pr(project_root, &head, &base) {
                        PrPresence::Open { number } => {
                            steps.push((
                                step,
                                StepOutcome::InFlight {
                                    detail: format!(
                                        "pull request #{number} is already open ({head} -> {base})"
                                    ),
                                },
                            ));
                            break;
                        }
                        PrPresence::Unreachable { reason } => {
                            steps.push((step, StepOutcome::Stopped { reason }));
                            break;
                        }
                        PrPresence::None => {
                            // Fall through to the step's own action, below.
                        }
                    }
                }

                match action_for(step) {
                    None => {
                        steps.push((
                            step,
                            StepOutcome::NoActionInThisBuild {
                                unit: unit_for(step),
                            },
                        ));
                    }
                    Some(action) => match action(project_root, version) {
                        Ok(detail) => steps.push((step, StepOutcome::Performed { detail })),
                        Err(reason) => steps.push((step, StepOutcome::Stopped { reason })),
                    },
                }
                break;
            }
        }
    }

    CutReport {
        steps,
        authorized: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn present(detail: &str) -> Observation {
        Observation::Present {
            detail: detail.into(),
        }
    }

    fn absent(detail: &str) -> Observation {
        Observation::Absent {
            detail: detail.into(),
        }
    }

    fn unreachable(reason: &str) -> Observation {
        Observation::Unreachable {
            reason: reason.into(),
        }
    }

    // -- StepOutcome::is_terminal ---------------------------------------

    #[test]
    fn is_terminal_true_for_every_variant_but_already_done() {
        assert!(!StepOutcome::AlreadyDone { detail: "x".into() }.is_terminal());
        assert!(StepOutcome::InFlight { detail: "x".into() }.is_terminal());
        assert!(StepOutcome::Performed { detail: "x".into() }.is_terminal());
        assert!(StepOutcome::Stopped { reason: "x".into() }.is_terminal());
        assert!(StepOutcome::NoActionInThisBuild { unit: "29b" }.is_terminal());
    }

    // -- CutReport::stopped_at / all_done ---------------------------------

    #[test]
    fn all_done_true_only_when_every_step_is_already_done() {
        let steps = ReleaseStep::ALL
            .iter()
            .map(|&s| {
                (
                    s,
                    StepOutcome::AlreadyDone {
                        detail: "done".into(),
                    },
                )
            })
            .collect();
        let report = CutReport {
            steps,
            authorized: true,
        };
        assert!(report.all_done());
        assert!(report.stopped_at().is_none());
    }

    #[test]
    fn stopped_at_returns_the_first_non_already_done_entry() {
        let report = CutReport {
            steps: vec![
                (
                    ReleaseStep::VersionBumped,
                    StepOutcome::AlreadyDone {
                        detail: "done".into(),
                    },
                ),
                (
                    ReleaseStep::ChangelogWritten,
                    StepOutcome::Stopped {
                        reason: "stopped here".into(),
                    },
                ),
            ],
            authorized: true,
        };
        assert!(!report.all_done());
        let (step, outcome) = report.stopped_at().expect("expected a stop");
        assert_eq!(step, ReleaseStep::ChangelogWritten);
        match outcome {
            StepOutcome::Stopped { reason } => assert_eq!(reason, "stopped here"),
            other => panic!("expected Stopped, got {other:?}"),
        }
    }

    // -- action_for / unit_for --------------------------------------------

    #[test]
    fn action_for_returns_none_for_every_step_in_this_build() {
        for step in ReleaseStep::ALL {
            assert!(
                action_for(step).is_none(),
                "expected no action for {step:?} in this build"
            );
        }
    }

    #[test]
    fn unit_for_maps_pr_backed_steps_to_29b_and_commit_point_steps_to_29c() {
        assert_eq!(unit_for(ReleaseStep::VersionBumped), "29b");
        assert_eq!(unit_for(ReleaseStep::ChangelogWritten), "29b");
        assert_eq!(unit_for(ReleaseStep::ReleasePrMerged), "29b");
        assert_eq!(unit_for(ReleaseStep::SyncMerged), "29b");
        assert_eq!(unit_for(ReleaseStep::SignedTagPresent), "29c");
        assert_eq!(unit_for(ReleaseStep::CratesPublished), "29c");
    }

    // -- pr_refs ------------------------------------------------------------

    #[test]
    fn pr_refs_is_deterministic_and_none_for_non_pr_backed_steps() {
        assert_eq!(
            pr_refs(ReleaseStep::VersionBumped, "1.2.3"),
            Some(("release/bump-v1.2.3".to_string(), "develop".to_string()))
        );
        assert_eq!(
            pr_refs(ReleaseStep::ChangelogWritten, "1.2.3"),
            Some(("release/bump-v1.2.3".to_string(), "develop".to_string()))
        );
        assert_eq!(
            pr_refs(ReleaseStep::ReleasePrMerged, "1.2.3"),
            Some(("develop".to_string(), "main".to_string()))
        );
        assert_eq!(
            pr_refs(ReleaseStep::SyncMerged, "1.2.3"),
            Some((
                "sync/main-to-develop-v1.2.3".to_string(),
                "develop".to_string()
            ))
        );
        assert_eq!(pr_refs(ReleaseStep::SignedTagPresent, "1.2.3"), None);
        assert_eq!(pr_refs(ReleaseStep::CratesPublished, "1.2.3"), None);
    }

    // -- cut_with: the walk's control flow --------------------------------

    #[test]
    fn present_then_absent_records_already_done_and_stops_on_absent() {
        // ChangelogWritten is pull-request-backed (`pr_refs` returns
        // `Some`), so the walk's own in-flight check runs a real `gh`
        // invocation against `/nonexistent` — which fails to even spawn
        // (an invalid `current_dir`), deterministically yielding
        // `PrPresence::Unreachable` regardless of the host's `gh`
        // installation. This test asserts on the walk's shape (stops on
        // the absent step), not on which terminal variant that failure
        // produces.
        let report = cut_with(
            Path::new("/nonexistent"),
            "1.2.3",
            true,
            |_, step, _| match step {
                ReleaseStep::VersionBumped => present("bumped"),
                ReleaseStep::ChangelogWritten => absent("not written"),
                _ => panic!("observer must not be called past the stop"),
            },
        );
        assert_eq!(report.steps.len(), 2);
        assert!(matches!(
            report.steps[0],
            (ReleaseStep::VersionBumped, StepOutcome::AlreadyDone { .. })
        ));
        assert_eq!(report.steps[1].0, ReleaseStep::ChangelogWritten);
        assert!(
            report.steps[1].1.is_terminal(),
            "the walk must stop on the absent step, got {:?}",
            report.steps[1].1
        );
    }

    #[test]
    fn unreachable_first_step_stops_immediately_with_the_reason_verbatim() {
        let report = cut_with(
            Path::new("/nonexistent"),
            "1.2.3",
            true,
            |_, step, _| match step {
                ReleaseStep::VersionBumped => unreachable("network partition"),
                _ => panic!("observer must not be called past an Unreachable stop"),
            },
        );
        assert_eq!(report.steps.len(), 1);
        match &report.steps[0] {
            (ReleaseStep::VersionBumped, StepOutcome::Stopped { reason }) => {
                assert_eq!(reason, "network partition");
            }
            other => panic!("expected Stopped naming the injected reason, got {other:?}"),
        }
    }

    #[test]
    fn a_stop_partway_through_records_no_outcome_for_later_steps() {
        // Stop at ReleasePrMerged (index 2, the third step): the report
        // must contain exactly index + 1 = 3 entries, and the observer must
        // never be consulted for SignedTagPresent, SyncMerged, or
        // CratesPublished.
        let report = cut_with(
            Path::new("/nonexistent"),
            "1.2.3",
            true,
            |_, step, _| match step {
                ReleaseStep::VersionBumped => present("bumped"),
                ReleaseStep::ChangelogWritten => present("written"),
                ReleaseStep::ReleasePrMerged => unreachable("gh api exited with status 1"),
                _ => panic!("observer must not be called for a step after the stop"),
            },
        );
        assert_eq!(
            report.steps.len(),
            3,
            "report length must equal the stop index (2) plus one"
        );
        assert!(matches!(
            report.steps[2],
            (ReleaseStep::ReleasePrMerged, StepOutcome::Stopped { .. })
        ));
    }

    #[test]
    fn unauthorized_yields_a_single_refusal_and_invokes_no_observer() {
        let observer_calls = RefCell::new(0u32);
        let report = cut_with(Path::new("/nonexistent"), "1.2.3", false, |_, _, _| {
            *observer_calls.borrow_mut() += 1;
            present("should never run")
        });
        assert_eq!(
            *observer_calls.borrow(),
            0,
            "authorized=false must not observe anything"
        );
        assert_eq!(report.steps.len(), 1);
        assert!(!report.authorized);
        match &report.steps[0].1 {
            StepOutcome::Stopped { reason } => {
                assert!(reason.contains("--yes-release"));
                assert!(reason.contains("yes_release"));
                assert!(reason.contains("DEVFLOW_YES_RELEASE"));
            }
            other => panic!("expected a mandate refusal, got {other:?}"),
        }
    }

    #[test]
    fn absent_step_with_no_pr_backing_and_no_action_stops_naming_the_supplying_unit() {
        // SignedTagPresent is not pull-request-backed (`pr_refs` returns
        // `None`), so the in-flight check is skipped entirely and the walk
        // goes straight to `action_for`, which is `None` in this build.
        let report = cut_with(
            Path::new("/nonexistent"),
            "1.2.3",
            true,
            |_, step, _| match step {
                ReleaseStep::VersionBumped => present("bumped"),
                ReleaseStep::ChangelogWritten => present("written"),
                ReleaseStep::ReleasePrMerged => present("merged"),
                ReleaseStep::SignedTagPresent => absent("no tag yet"),
                _ => panic!("observer must not be called past the stop"),
            },
        );
        assert_eq!(report.steps.len(), 4);
        match &report.steps[3] {
            (ReleaseStep::SignedTagPresent, StepOutcome::NoActionInThisBuild { unit }) => {
                assert_eq!(*unit, "29c");
            }
            other => panic!("expected NoActionInThisBuild naming 29c, got {other:?}"),
        }
    }

    /// A pull-request-backed step observed `Absent`, checked against a
    /// directory that is not even a git repository: `gh pr list` cannot
    /// resolve a repository from it, so the in-flight check itself is
    /// `Unreachable` regardless of whether `gh` happens to be installed —
    /// deterministic without any network or fixture dependency.
    #[test]
    fn absent_pr_backed_step_with_an_unreachable_in_flight_check_stops() {
        let dir = tempfile::tempdir().unwrap();
        let report = cut_with(dir.path(), "1.2.3", true, |_, step, _| match step {
            ReleaseStep::VersionBumped => absent("not bumped"),
            _ => panic!("observer must not be called past the stop"),
        });
        assert_eq!(report.steps.len(), 1);
        match &report.steps[0] {
            (ReleaseStep::VersionBumped, StepOutcome::Stopped { reason }) => {
                assert!(!reason.is_empty());
            }
            other => panic!("expected Stopped from an unreachable in-flight check, got {other:?}"),
        }
    }

    /// End-to-end against a real repository with no remote at all: `cut`'s
    /// public entry point (not `cut_with`) runs the real observer, and the
    /// first step's failure carries git's/gh's own real failure text
    /// rather than a predicted one.
    #[test]
    fn cut_authorized_against_a_repo_with_no_remote_refuses_on_the_first_step() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let output = crate::test_support::git_command(root)
                .args(args)
                .output()
                .expect("spawn git");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        git(&["checkout", "-q", "-b", "develop"]);

        let report = cut(root, "1.2.3", true);
        assert_eq!(
            report.steps.len(),
            1,
            "expected the walk to stop on the very first step with no remote"
        );
        match &report.steps[0] {
            (ReleaseStep::VersionBumped, StepOutcome::Stopped { reason }) => {
                assert!(
                    !reason.is_empty(),
                    "expected a real, non-empty failure reason"
                );
            }
            other => panic!("expected Stopped on the first step, got {other:?}"),
        }
    }
}
