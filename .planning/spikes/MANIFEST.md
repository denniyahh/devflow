# Spike Manifest

## Ideas

### planning-repo-topology
Where `.planning/` (and personal agent/environment config) should live relative to the code
repository. Today it is tracked on a personal branch that phase branches fork from, and hooks plus a
filtering script keep it out of `develop` (option A). The alternative is a separate spec repository
holding `.planning/`, with the code repository as a child directory, which GSD documents as
`planning.sub_repos` (option B). A middle path has DevFlow automate option A. The choice decides
whether `devflow.toml` (999.110) holds one location or two, and whether DevFlow's Ship stage can run
end to end to `develop`.

**Requirements:**
- `.planning/**` never reaches `develop` or `main` (operator, 2026-08-23; enforced today by
  `scripts/hooks/pre-push`).
- No runtime copy or symlink of `.planning/` into a worktree forked from `develop` (operator,
  2026-08-23, recorded in 999.110). Revisit only if option B makes `.planning/` a separate repo,
  where the reason for that rule no longer applies.
- DevFlow's location settings live in `devflow.toml` (operator, 2026-09-18, 999.110).

## Spikes

| # | Idea | Name | Type | Validates | Verdict | Tags |
|---|------|------|------|-----------|---------|------|
| 001 | planning-repo-topology | planning-repo-topology | standard | Given a spec repo holding `.planning/` with the code repo as a child directory, when GSD plans and executes one phase and DevFlow runs one stage, then planning commits land in the spec repo, code commits land in the code repo, and DevFlow's checks can find both | PENDING — not run (operator deferred the trial 2026-09-18) | gsd, devflow, git, worktrees, sub_repos, planning |
