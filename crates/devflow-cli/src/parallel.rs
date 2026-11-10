//! Multi-phase orchestration: spawning several phases concurrently, each in
//! its own worktree (`parallel`).

use devflow_core::mode::Mode;
use devflow_core::state::AgentKind;
use std::path::Path;

use crate::CliError;
use crate::commands::{FallbackSource, start};
use devflow_core::phase_id::PhaseId;

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
    // `parallel` arms no fallback (see `NoFallback` below), but a configured key is
    // still read once, before anything launches: a key that names no agent, or a file
    // that does not parse, refuses `parallel` as it refuses `start`, and a valid key
    // gets a note that it is not armed. Ignoring it silently would leave the operator
    // believing a fallback is armed (`config::fallback_agent`'s reason to fail hard).
    if let Some(fallback) =
        devflow_core::config::fallback_agent(project_root).map_err(CliError::Message)?
    {
        println!(
            "note: devflow.toml's `fallback_agent` ({fallback}) is not armed by `devflow \
             parallel`; its runs carry no fallback. To fail over a phase, run it with \
             `devflow start --phase N --fallback-agent {fallback}`, or re-arm a stopped or \
             paused run with `devflow resume --phase N --fallback-agent {fallback}`"
        );
    }
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
        // The trailing `NoFallback` is the fallback agent (51-04): `parallel` takes
        // no `--fallback-agent` and does not arm `devflow.toml`'s `fallback_agent`
        // key (it only checks and names it, above), so its runs carry none.
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
            FallbackSource::NoFallback,
        )?;
    }
    Ok(())
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
}
