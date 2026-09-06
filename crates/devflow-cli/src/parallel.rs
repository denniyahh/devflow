//! Multi-phase orchestration: spawning several phases concurrently, each in
//! its own worktree (`parallel`).

use devflow_core::config::FEATURE_PREFIX;
use devflow_core::git::GitFlow;
use devflow_core::mode::Mode;
use devflow_core::state::AgentKind;
use devflow_core::worktree;
use std::path::{Path, PathBuf};

use crate::CliError;
use crate::commands::start;
use devflow_core::phase_id::PhaseId;

/// Create the phase worktree at `.worktrees/phase-NN/` on `feature/phase-NN`,
/// forked from `base`.
///
/// `base` is the project-resolved value from `config::base_branch` (D-01 /
/// AUTO-01), not the built-in trunk: a project whose `.planning/` lives on a
/// planning branch needs the worktree to carry it, or
/// `preflight_unattended_launch_check` refuses every unattended launch.
/// Callers must pass the same value the reachability and currency guards
/// inspected, so the branch checked and the branch forked can never disagree.
pub(crate) fn ensure_phase_worktree(
    project_root: &Path,
    phase: PhaseId,
    force: bool,
    base: &str,
) -> Result<PathBuf, CliError> {
    let wt = worktree::phase_path(project_root, phase);
    let branch = format!("{FEATURE_PREFIX}phase-{padded}", padded = phase.padded());

    if force {
        if wt.exists() {
            worktree::remove(project_root, &wt, true)?;
        }
        // Project-resolved: `delete_branch`'s protected-branch list must name
        // the configured trunk, not the default one.
        let _ = GitFlow::for_project(project_root).delete_branch(&branch, true);
    }

    match worktree::add(project_root, &wt, &branch, base, true) {
        Ok(()) => Ok(wt),
        Err(devflow_core::worktree::WorktreeError::Exists(path)) => {
            Err(CliError::Message(format!(
                "worktree already exists at {} — use --force to recreate it",
                path.display()
            )))
        }
        Err(err) => Err(err.into()),
    }
}

// ---------------------------------------------------------------------------
// parallel
// ---------------------------------------------------------------------------

/// Parse `--phases` and optional `--agents` into positional (phase, agent)
/// pairs. Agents default to `claude` when fewer are given than phases; an error
/// is returned when more agents than phases are supplied.
fn parse_phase_agent_pairs(
    phases: &str,
    agents: Option<&str>,
) -> Result<Vec<(PhaseId, AgentKind)>, CliError> {
    let phases: Vec<PhaseId> = phases
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<PhaseId>()
                .map_err(|err| CliError::Message(err.to_string()))
        })
        .collect::<Result<_, _>>()?;
    if phases.is_empty() {
        return Err(CliError::Message("no phases given".into()));
    }

    let agents: Vec<AgentKind> = match agents {
        Some(list) => list
            .split(',')
            .map(|a| a.trim())
            .filter(|a| !a.is_empty())
            .map(|a| {
                a.parse::<AgentKind>()
                    .map_err(|err| CliError::Message(err.to_string()))
            })
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    if agents.len() > phases.len() {
        return Err(CliError::Message(format!(
            "got {} agents for {} phases — provide at most one agent per phase",
            agents.len(),
            phases.len()
        )));
    }

    Ok(phases
        .into_iter()
        .enumerate()
        .map(|(i, phase)| (phase, agents.get(i).copied().unwrap_or(AgentKind::Claude)))
        .collect())
}

/// Spawn one monitored worktree run per phase, concurrently.
pub(crate) fn parallel(
    project_root: &Path,
    phases: &str,
    agents: Option<&str>,
    mode: Mode,
    force: bool,
) -> Result<(), CliError> {
    let pairs = parse_phase_agent_pairs(phases, agents)?;
    println!("launching {} phase(s) in parallel worktrees", pairs.len());
    for (phase, agent) in pairs {
        println!("\n=== phase {phase} ({agent}) ===");
        // Worktree mode keeps each run isolated so the phases run together.
        // `devflow parallel` has no `--yes-ship` flag of its own (D-05: the
        // pre-authorization must be typed per invocation on `devflow
        // start`), so every phase it launches keeps the routine gated Ship
        // behavior — `false` here changes nothing about `parallel`'s
        // existing behavior.
        //
        // The trailing `false` is D-11's legacy-launch opt-out, off for the
        // same reason and one stronger: `parallel` is precisely the multi-run
        // shape whose delegated work the legacy path orphans (999.64). An
        // operator who genuinely wants it can still set
        // `DEVFLOW_CLAUDE_LEGACY_LAUNCH`, which `start` reads per phase.
        start(
            project_root,
            phase,
            agent,
            mode,
            force,
            true,
            false,
            None,
            false,
            false,
        )?;
    }
    Ok(())
}

pub(crate) fn retry_after_from_reason(reason: Option<&str>) -> String {
    reason
        .and_then(|s| s.strip_prefix("rate limited until "))
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_default_missing_agents_to_claude() {
        let pairs = parse_phase_agent_pairs("7,8", Some("codex")).unwrap();
        assert_eq!(
            pairs,
            vec![
                (PhaseId::new(7), AgentKind::Codex),
                (PhaseId::new(8), AgentKind::Claude)
            ]
        );
    }

    #[test]
    fn pairs_match_agents_positionally() {
        let pairs = parse_phase_agent_pairs("7, 8", Some("claude, codex")).unwrap();
        assert_eq!(
            pairs,
            vec![
                (PhaseId::new(7), AgentKind::Claude),
                (PhaseId::new(8), AgentKind::Codex)
            ]
        );
    }

    #[test]
    fn pairs_default_all_to_claude_without_agents() {
        let pairs = parse_phase_agent_pairs("3,4", None).unwrap();
        assert_eq!(
            pairs,
            vec![
                (PhaseId::new(3), AgentKind::Claude),
                (PhaseId::new(4), AgentKind::Claude)
            ]
        );
    }

    #[test]
    fn pairs_reject_more_agents_than_phases() {
        let err = parse_phase_agent_pairs("7", Some("claude,codex")).unwrap_err();
        assert!(matches!(err, CliError::Message(_)));
    }

    #[test]
    fn pairs_reject_invalid_phase() {
        assert!(parse_phase_agent_pairs("7,x", None).is_err());
        assert!(parse_phase_agent_pairs("", None).is_err());
    }

    /// 45-01 / AUTO-01: the fork point is the value the caller supplies, not
    /// the built-in trunk. BOTH directions are asserted in this one body —
    /// a test that only checked the configured base would pass against a
    /// function that ignored its argument in a repository where `develop`
    /// happened to carry the same file.
    #[test]
    fn ensure_phase_worktree_forks_from_the_supplied_base() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = |args: &[&str]| {
            let out = devflow_core::test_support::git_command(root)
                .args(args)
                .output()
                .expect("spawn git");
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q", "-b", "develop"]);
        git(&["config", "user.email", "t@e.st"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["config", "core.hooksPath", "/dev/null"]);
        std::fs::write(root.join("README.md"), "x").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);

        // A planning branch carrying a file that is absent on `develop` —
        // the `.planning/config.json` shape AUTO-01 is about.
        git(&["checkout", "-q", "-b", "workspace/example"]);
        std::fs::create_dir_all(root.join(".planning")).unwrap();
        std::fs::write(root.join(".planning/config.json"), "{}").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "planning"]);
        git(&["checkout", "-q", "develop"]);

        let wt = ensure_phase_worktree(root, PhaseId::new(45), false, "workspace/example")
            .expect("worktree off the configured base");
        assert!(
            wt.join(".planning/config.json").exists(),
            "a worktree forked from the configured base must carry its files"
        );

        // NEGATIVE CONTROL: the same call against `develop` must NOT see it.
        let wt_default =
            ensure_phase_worktree(root, PhaseId::new(46), false, devflow_core::config::DEVELOP)
                .expect("worktree off develop");
        assert!(
            !wt_default.join(".planning/config.json").exists(),
            "a worktree forked from develop must not carry the planning branch's files"
        );
    }

    #[test]
    fn retry_after_from_reason_strips_prefix() {
        assert_eq!(
            retry_after_from_reason(Some("rate limited until 2026-06-18T15:45:30Z")),
            "2026-06-18T15:45:30Z"
        );
        assert_eq!(retry_after_from_reason(Some("usage limit")), "unknown");
        assert_eq!(retry_after_from_reason(None), "unknown");
    }
}
