//! Multi-phase orchestration: spawning and integrating several phases at
//! once (`parallel`), and the `sequentagent` blocking two-agent handoff path.
//!
//! `run_agent_blocking` operates on synthetic, never-persisted state —
//! `sequentagent` does not participate in the stage machine, so there is no
//! `save_state` chokepoint here (D-14: the retrospective proposal to add one
//! was verified wrong for exactly this reason before this phase's split).

use devflow_core::agent_result::{self, AgentStatus};
use devflow_core::agents;
use devflow_core::config::{DEVELOP, FEATURE_PREFIX, capture_retention};
use devflow_core::events;
use devflow_core::git::GitFlow;
use devflow_core::lock;
use devflow_core::mode::Mode;
use devflow_core::monitor;
use devflow_core::prompt;
use devflow_core::stage::Stage;
use devflow_core::state::{AgentKind, State};
use devflow_core::worktree;
use std::path::{Path, PathBuf};

use crate::preflight::{agent_program, ensure_agent_binary, worktree_writable_roots};
use crate::{CliError, checkout_lock_timeout, start};

/// Create the phase worktree at `.worktrees/phase-NN/` on `feature/phase-NN`.
pub(crate) fn ensure_phase_worktree(
    project_root: &Path,
    phase: u32,
    force: bool,
) -> Result<PathBuf, CliError> {
    let wt = worktree::phase_path(project_root, phase);
    let branch = format!("{FEATURE_PREFIX}phase-{phase:02}");

    if force {
        if wt.exists() {
            worktree::remove(project_root, &wt, true)?;
        }
        let _ = GitFlow::new(project_root).delete_branch(&branch, true);
    }

    match worktree::add(project_root, &wt, &branch, DEVELOP, true) {
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
// parallel / sequentagent
// ---------------------------------------------------------------------------

/// Parse `--phases` and optional `--agents` into positional (phase, agent)
/// pairs. Agents default to `claude` when fewer are given than phases; an error
/// is returned when more agents than phases are supplied.
fn parse_phase_agent_pairs(
    phases: &str,
    agents: Option<&str>,
) -> Result<Vec<(u32, AgentKind)>, CliError> {
    let phases: Vec<u32> = phases
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<u32>()
                .map_err(|_| CliError::Message(format!("invalid phase number `{p}`")))
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
        start(project_root, phase, agent, mode, force, true, false)?;
    }
    Ok(())
}

/// Parse exactly two comma-separated agents for `sequentagent`.
fn split_two_agents(agents: &str) -> Result<(AgentKind, AgentKind), CliError> {
    let parsed: Vec<AgentKind> = agents
        .split(',')
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .map(|a| {
            a.parse::<AgentKind>()
                .map_err(|err| CliError::Message(err.to_string()))
        })
        .collect::<Result<_, _>>()?;
    if parsed.len() != 2 {
        return Err(CliError::Message(format!(
            "sequentagent requires exactly two agents (e.g. claude,codex), got {}",
            parsed.len()
        )));
    }
    Ok((parsed[0], parsed[1]))
}

/// Launch one agent via a no-advance monitor, block until it exits, and
/// return its self-reported result (parsed from the DEVFLOW_RESULT marker,
/// if present). Used by sequentagent, where the rebase handoff between
/// agents requires a synchronous run.
///
/// 14b: this used to be a CLI-owned `launch_agent` + `capture_agent_output`
/// pipe — the last synchronous execution path. It now rides the same
/// monitor-owned execution as everything else (stderr separated from the
/// parseable stdout capture, exit code reaped even if this CLI dies) and
/// simply blocks on the exit file the monitor writes.
fn run_agent_blocking(
    project_root: &Path,
    phase: u32,
    agent: AgentKind,
    workdir: &Path,
) -> Result<Option<agent_result::AgentResult>, CliError> {
    if let Some(stamp) = agent_result::archive_phase_files(
        project_root,
        workdir,
        phase,
        capture_retention(project_root),
    )
    .map_err(|err| {
        CliError::Message(format!(
            "could not archive phase {phase} capture before rollover: {err}"
        ))
    })? {
        events::emit(
            project_root,
            phase,
            "capture_archived",
            serde_json::json!({"stage": "code", "stamp": stamp}),
        );
    }
    let adapter = agents::adapter_for(agent);
    let prompt = prompt::stage_prompt_for_project(Stage::Code, phase, project_root);
    // sequentagent always runs in a worktree — the main repo's `.git/` and
    // the worktree admin dir must stay writable for sandboxed agents to
    // commit (13-06 dogfood finding).
    let roots = if workdir == project_root {
        Vec::new()
    } else {
        worktree_writable_roots(project_root, workdir)
    };
    let (program, args) = adapter.exec_command(phase, &prompt, &roots);
    ensure_agent_binary(program)?;
    // Synthetic, never-persisted state: the monitor only reads project_root,
    // phase, and worktree_path from it — sequentagent does not participate
    // in the stage machine.
    let mut state = State::new(phase, agent, Mode::Auto, project_root.to_path_buf());
    state.stage = Stage::Code;
    if workdir != project_root {
        state.worktree_path = Some(workdir.to_path_buf());
    }
    let monitor_pid =
        monitor::spawn_monitor_no_advance(&state, program, &args, &adapter.extra_env())
            .map_err(|err| CliError::Message(format!("could not spawn monitor: {err}")))?;
    println!(
        "launched {} (monitor pid {monitor_pid}) in {}",
        adapter.name(),
        workdir.display()
    );
    // 14-CR-09: the sync path used to stream agent stderr to this terminal;
    // the monitor captures it instead — tell the operator where to watch.
    println!("  watch live: devflow logs -f --phase {phase} [--stderr]");
    let exit_code = monitor::wait_for_agent_exit(project_root, phase, monitor_pid)
        .map_err(|err| CliError::Message(format!("agent run did not complete: {err}")))?;
    println!("agent {agent} exited with code {exit_code}");
    // The monitor wrote stdout to the same file evaluate_layer1 reads, so
    // delegate to it directly rather than re-implementing a subset of its
    // precedence here — this is the single code path that knows how to find
    // a Codex agent's DEVFLOW_RESULT marker inside its JSONL `--json` event
    // stream (parse_codex_event_result).
    let result = agent_result::evaluate_layer1(project_root, phase);
    // Layer-1 silence is not success: a crashed agent (nonzero exit, no
    // marker, no envelope) yields None here, and sequentagent's callers
    // treat None as "proceed to integrate". Mirror Layer 2's exit-code gate
    // so a crash never fast-forwards a half-finished branch into the base.
    if result.is_none() && exit_code != 0 {
        return Ok(Some(agent_result::AgentResult {
            status: AgentStatus::Failed,
            exit_code: Some(exit_code),
            reason: Some(format!(
                "agent exited with code {exit_code} without reporting a result"
            )),
            commits: None,
            summary: None,
            verdict: None,
            // Mirrors Layer 2's exit-code gate per this block's own comment
            // (review consensus #3).
            decided_by_layer: Some(2),
        }));
    }
    Ok(result)
}

/// Integrate an agent branch into the shared base, pushing if a remote
/// exists. Serialized on the coarse checkout lock: branch fast-forwards and
/// pushes mutate shared refs that a concurrently finishing phase's hooks
/// also touch (13-DEFERRED-CR-03 sequentagent re-check).
fn integrate_agent_branch(
    project_root: &Path,
    git: &GitFlow,
    base: &str,
    agent_branch: &str,
) -> Result<(), CliError> {
    // 14-CR-07: this hard-fails on a lock timeout by design (a shared-ref
    // mutation must never run unlocked), which can leave an earlier agent's
    // branch integrated and this one not — so the error carries resume
    // guidance instead of leaving the operator to guess (re-running with
    // --force would re-run agents on top of already-integrated work).
    let _checkout_lock = lock::acquire_project_blocking(project_root, checkout_lock_timeout())
        .map_err(|err| {
            CliError::Message(format!(
                "could not lock checkout to integrate {agent_branch} into {base}: {err}. \
                 Earlier integrations into {base} are already in place; once the lock \
                 holder finishes, integrate manually with \
                 `git fetch . {agent_branch}:{base}` — do NOT re-run sequentagent --force."
            ))
        })?;
    git.fast_forward_branch(base, agent_branch)?;
    println!("integrated {agent_branch} into {base}");
    if git.has_remote() {
        match git.push(base) {
            Ok(()) => println!("pushed {base} to origin"),
            Err(err) => println!("warning: could not push {base}: {err}"),
        }
    }
    Ok(())
}

/// Run two agents sequentially on one phase, each in its own worktree, with a
/// rebase handoff between them. See the `Sequentagent` command docs.
pub(crate) fn sequentagent(
    project_root: &Path,
    phase: u32,
    agents: &str,
    force: bool,
) -> Result<(), CliError> {
    // 14b: hold this phase's lock for the whole run — a monitored pipeline
    // run and a sequentagent run for the same phase share capture files and
    // branches, and previously nothing excluded them from colliding.
    let _phase_lock = match lock::acquire(project_root, phase) {
        Ok(guard) => guard,
        Err(lock::LockError::Contended { pid, path: _ }) => {
            return Err(CliError::Message(format!(
                "phase {phase} is already being driven by another devflow process (pid {pid})"
            )));
        }
        Err(err) => return Err(CliError::Message(format!("lock error: {err}"))),
    };
    if let Err(err) = devflow_core::ship::delete_cron_instructions(project_root, phase) {
        println!("warning: could not remove stale cron-instructions file: {err}");
    }
    let (agent_a, agent_b) = split_two_agents(agents)?;
    // 14-CR-05: both binaries must resolve before any branch/worktree is
    // scaffolded — agent B's absence should not surface after A's full run.
    ensure_agent_binary(agent_program(agent_a))?;
    ensure_agent_binary(agent_program(agent_b))?;
    let git = GitFlow::new(project_root);
    let base = format!("{FEATURE_PREFIX}phase-{phase:02}");

    // 1. Ensure the shared base branch exists off develop, without leaving the
    //    main checkout on it. Ref creation is serialized on the checkout lock
    //    like every other shared-ref mutation.
    {
        let _checkout_lock = lock::acquire_project_blocking(project_root, checkout_lock_timeout())
            .map_err(|err| CliError::Message(format!("could not lock checkout: {err}")))?;
        git.ensure_branch(&base, DEVELOP)?;
    }

    // 2. Create one worktree per agent, both off the current base tip.
    let branch_a = format!("{base}-{agent_a}");
    let branch_b = format!("{base}-{agent_b}");
    let wt_a = worktree::phase_agent_path(project_root, phase, &agent_a.to_string());
    let wt_b = worktree::phase_agent_path(project_root, phase, &agent_b.to_string());

    if force {
        for (wt, branch) in [(&wt_a, &branch_a), (&wt_b, &branch_b)] {
            if wt.exists() {
                worktree::remove(project_root, wt, true)?;
            }
            let _ = git.delete_branch(branch, true);
        }
    }

    add_or_explain(project_root, &wt_a, &branch_a, &base)?;
    add_or_explain(project_root, &wt_b, &branch_b, &base)?;
    println!("worktree A: {} ({branch_a})", wt_a.display());
    println!("worktree B: {} ({branch_b})", wt_b.display());

    // 3. Run agent A; stop before touching B if it fails.
    println!("\n=== agent A: {agent_a} ===");
    if let Some(result) = run_agent_blocking(project_root, phase, agent_a, &wt_a)? {
        match result.status {
            AgentStatus::Failed => {
                return Err(CliError::Message(format!(
                    "agent A ({agent_a}) failed: {}",
                    result.reason.unwrap_or_else(|| "no details".into())
                )));
            }
            AgentStatus::RateLimited => {
                let retry_after = retry_after_from_reason(result.reason.as_deref());
                write_rate_limit_cron(project_root, phase, &retry_after, agents)?;
                let commits = count_commits_between(project_root, &base, &branch_a)?;
                if commits == 0 {
                    println!(
                        "Agent A rate-limited with zero commits; paused — resume record at {}",
                        devflow_core::ship::cron_instructions_path(project_root, phase).display()
                    );
                    return Ok(());
                }
                println!("Agent A rate-limited; handing off to agent B");
            }
            _ => {}
        }
    }
    integrate_agent_branch(project_root, &git, &base, &branch_a)?;

    // 4. Rebase B onto the updated base; surface conflicts for manual fixing.
    git.rebase_in(&wt_b, &base).map_err(|err| {
        CliError::Message(format!(
            "rebase of {branch_b} onto {base} hit conflicts — resolve them in {} \
             then re-run sequentagent: {err}",
            wt_b.display()
        ))
    })?;
    println!("rebased {branch_b} onto {base}");

    // 5. Run agent B and integrate.
    println!("\n=== agent B: {agent_b} ===");
    if let Some(result) = run_agent_blocking(project_root, phase, agent_b, &wt_b)?
        && matches!(
            result.status,
            AgentStatus::Failed | AgentStatus::RateLimited
        )
    {
        let label = if result.status == AgentStatus::RateLimited {
            "rate-limited"
        } else {
            "failed"
        };
        return Err(CliError::Message(format!(
            "agent B ({agent_b}) {label}: {}",
            result.reason.unwrap_or_else(|| "no details".into())
        )));
    }
    integrate_agent_branch(project_root, &git, &base, &branch_b)?;
    // WR-02: the phase has shipped — a surviving cron-instructions file would
    // mislead `devflow status`/a Hermes cron into re-running it. A failed
    // delete must be visible, not swallowed, for the same reason.
    if let Err(err) = devflow_core::ship::delete_cron_instructions(project_root, phase) {
        println!("warning: could not remove cron-instructions file: {err}");
    }

    println!("\nsequentagent complete — both agents integrated into {base}");
    Ok(())
}

pub(crate) fn retry_after_from_reason(reason: Option<&str>) -> String {
    reason
        .and_then(|s| s.strip_prefix("rate limited until "))
        .unwrap_or("unknown")
        .to_string()
}

fn write_rate_limit_cron(
    project_root: &Path,
    phase: u32,
    retry_after: &str,
    agents: &str,
) -> Result<(), CliError> {
    let instructions =
        devflow_core::ship::build_cron_instructions(project_root, phase, retry_after, agents);
    devflow_core::ship::write_cron_instructions(project_root, &instructions)?;
    if instructions.hermes_cron.schedule.is_empty() {
        println!("no parseable retry time — auto-resume cron not scheduled; resume manually");
    } else {
        // 14-CR-08: name the file that was actually written (per-phase),
        // not the retired single-slot path.
        println!(
            "wrote {}",
            devflow_core::ship::cron_instructions_path(project_root, phase)
                .strip_prefix(project_root)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| {
                    devflow_core::ship::cron_instructions_path(project_root, phase)
                        .display()
                        .to_string()
                })
        );
    }
    Ok(())
}

fn count_commits_between(project_root: &Path, base: &str, branch: &str) -> Result<u32, CliError> {
    let range = format!("{base}..{branch}");
    let output = std::process::Command::new("git")
        .args(["rev-list", "--count", &range])
        .current_dir(project_root)
        .output()
        .map_err(|err| CliError::Message(format!("could not count commits on {branch}: {err}")))?;
    if !output.status.success() {
        return Err(CliError::Message(format!(
            "could not count commits on {branch}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|err| CliError::Message(format!("invalid commit count for {branch}: {err}")))
}

/// Add a worktree, turning the "already exists" error into actionable advice.
fn add_or_explain(
    project_root: &Path,
    path: &Path,
    branch: &str,
    base: &str,
) -> Result<(), CliError> {
    match worktree::add(project_root, path, branch, base, true) {
        Ok(()) => Ok(()),
        Err(devflow_core::worktree::WorktreeError::Exists(p)) => Err(CliError::Message(format!(
            "worktree already exists at {} — use --force to recreate it",
            p.display()
        ))),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_default_missing_agents_to_claude() {
        let pairs = parse_phase_agent_pairs("7,8", Some("codex")).unwrap();
        assert_eq!(pairs, vec![(7, AgentKind::Codex), (8, AgentKind::Claude)]);
    }

    #[test]
    fn pairs_match_agents_positionally() {
        let pairs = parse_phase_agent_pairs("7, 8", Some("claude, codex")).unwrap();
        assert_eq!(pairs, vec![(7, AgentKind::Claude), (8, AgentKind::Codex)]);
    }

    #[test]
    fn pairs_default_all_to_claude_without_agents() {
        let pairs = parse_phase_agent_pairs("3,4", None).unwrap();
        assert_eq!(pairs, vec![(3, AgentKind::Claude), (4, AgentKind::Claude)]);
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

    #[test]
    fn split_two_agents_requires_exactly_two() {
        assert_eq!(
            split_two_agents("claude, codex").unwrap(),
            (AgentKind::Claude, AgentKind::Codex)
        );
        assert!(split_two_agents("claude").is_err());
        assert!(split_two_agents("claude,codex,opencode").is_err());
        assert!(split_two_agents("claude,bogus").is_err());
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
