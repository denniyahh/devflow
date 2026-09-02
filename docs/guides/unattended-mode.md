# Unattended Mode

`devflow start --mode auto` is for the run you start and walk away from. This
page is what you should read before you do that: what the mode actually
automates, what DevFlow checks before it will start one, what it writes into
your git history, and the two places an unattended run can still stop.

## What unattended mode does

For the duration of the **Code** stage — and of the `--gaps-only` fix loop that
a failing Validate sends back to Code — DevFlow sets GSD's chain flag,
`workflow._auto_chain_active`, in your project's `.planning/config.json`. GSD
reads that flag when a plan reaches an ordinary blocking checkpoint. With the
flag set, GSD approves the checkpoint and keeps going instead of stopping to
ask you.

The flag is set when the stage launches and cleared when it finishes, on every
exit path including a failure.

**Name the boundary plainly: this is checkpoint approval only.** Two things it
is deliberately *not*:

- **It is not stage-chaining.** DevFlow still drives the pipeline itself,
  one stage at a time. GSD is never told to launch the next stage on its own.
- **It never writes `workflow.auto_advance`.** That is your persistent
  settings preference, and DevFlow does not touch it. Only the ephemeral
  `_auto_chain_active` key is ever written, and only that one key.

Checkpoints marked `blocking-human`, and `checkpoint:human-action` tasks, are
never auto-approved by any mode. A phase whose plans declare one is refused
before it starts — see the next section.

## Before you start: what the preflight checks

An unattended launch is checked before any agent is spawned. Three conditions
are evaluated and **all three are reported every time**, in both `auto` and
`supervise` mode, so you can rehearse a run in supervise and see exactly what
an auto run would say. Only `--mode auto` is refused on a failure.

Each condition reports one of `[holds]`, `[DOES NOT HOLD]`,
`[COULD NOT BE DETERMINED]`, or `[not yet applicable]`. In auto mode, anything
that is not a definite pass refuses — including *could not determine*.
Unreadable is not the same as absent, and neither is treated as fine.

### 1. The GSD config can hold the chain flag

`.planning/config.json` must exist under the launch root, parse, and be a JSON
object. If it is missing there is nowhere for the flag to live; if it does not
parse, DevFlow cannot tell whether writing to it would destroy something.

A refusal here means your project is not GSD-configured, or its config file is
malformed. Fix the file, or run in supervise mode.

### 2. The Code stage would launch on the pipe-owning arm

The guard that bounds the flag's lifetime lives inside DevFlow's pipe-owning
monitor, and only the Claude launch path starts one. So two launch shapes are
refused in auto mode:

- **A non-Claude agent.** `--agent codex` and `--agent opencode` cannot run
  `--mode auto`. This is a real capability removal introduced with this
  feature; those agents still work in supervise mode.
- **The legacy launch opt-out.** `--legacy-claude-launch` (or
  `DEVFLOW_CLAUDE_LEGACY_LAUNCH`) selects a detached shell monitor with no
  process to bound the flag's lifetime. It is now mutually exclusive with
  `--mode auto`.

The recovery for either is the same: drop the flag that caused it, or run in
supervise mode.

### 3. No plan declares a checkpoint GSD never auto-approves

DevFlow scans the phase's `*-PLAN.md` files for a `blocking-human` gate or a
`checkpoint:human-action` task declared on a task tag. Those are human-only by
design and no mode approves them, so an unattended run that reached one would
simply stall. Better to say so at the start.

Before the phase has been planned there is nothing to scan, and the condition
reports `[not yet applicable]` at Define rather than refusing. At Code, plans
are expected — their absence is `[COULD NOT BE DETERMINED]`, and that refuses.

A refusal here means replanning the phase without the human-only marker, or
running in supervise mode and answering it yourself.

### A refusal is final

There is no `--force-unattended` flag and no environment variable that turns a
refusal into a warning. Your options are to fix the condition or to run in
supervise mode.

**Both external review lanes objected to that, on record, and the decision
stands.** The objection is real and worth stating so you know it was heard: a
preflight that false-positives makes unattended runs impossible with no
in-product recovery, and one reviewer characterised that as a
denial-of-service. The counter-argument that carried is DevFlow's standing
principle — advance as far as the rules permit and stop at the first hard gate,
never route around one — plus the asymmetry that adding an override later is
easy while removing one after operators have built habits around it is not. If
the preflight's own reliability turns out to be a problem in practice, that is
grounds to reopen the decision, not to work around it.

### If your planning artifacts do not live on `develop`

The first preflight condition reads `.planning/config.json` **from the phase
worktree**, and that worktree is forked from your project's base branch. If
your `.planning/` lives on a branch other than `develop` — a personal tracking
branch, say — a worktree forked from `develop` does not carry it, the condition
cannot hold, and every `--mode auto` launch is refused with nothing you can fix
inside the run.

Set the base branch and the problem goes away:

```toml
# devflow.toml
base_branch = "workspace/yourname"
```

or export `DEVFLOW_BASE_BRANCH`, which outranks the file.

Three things to know before you set it:

- **It is the whole trunk, not just a start point.** Phase worktrees fork
  *from* this branch and the git-flow lifecycle merges phase work back *into*
  it. Both resolve from the one value, so they cannot drift apart. The
  alternative — forking from your planning branch and merging into `develop` —
  would drag unrelated history into the integration branch.
- **`main` is refused.** So is a blank value and anything beginning with `-`.
  An explicitly configured bad value is a hard error naming the value and where
  it came from, never a quiet fallback to `develop`.
- **It must be an existing local branch.** A remote-tracking name
  (`origin/foo`), a `refs/heads/` path, `HEAD`, or a commit SHA is refused,
  because a merge target has to be a branch.

Leave it unset and nothing changes: the base is `develop`, exactly as before.

## What you will see in your git history

The chain flag lives in `.planning/config.json`, which is a **tracked** file.
So:

- **During the Code stage, that file is modified in your working tree.** The
  value flips to `true` when the stage launches and back to `false` when it
  finishes. Only that one key changes — key order, formatting, and every other
  setting are preserved byte-for-byte.

- **If a run is killed, the flag can be left set.** A `SIGKILL` gives DevFlow's
  in-process guard no chance to run. The next `devflow start` or
  `devflow resume` for that phase detects the leftover value and clears it
  before launching anything, printing a notice as it does.

- **That repair may write a commit.** If the leaked `true` had already reached
  the branch tip, the repair commits the corrected file so it cannot travel
  onward through Ship into `develop`. You will see this subject in your
  history:

  ```
  fix(gsd): clear a leaked auto-chain flag before launch
  ```

  The body explains what the leaked value meant. The repair is also recorded as
  an `auto_chain_flag_repaired` entry in `.devflow/events.jsonl`.

- **The repair declines to commit if you edited that file too.** If
  `.planning/config.json` carries changes of yours beyond the flag, DevFlow
  clears the flag in your working tree, refuses to make the commit, and says so
  loudly. It will not sweep your edit into a commit you did not ask for.

## Known limitations

Two, and neither is fixed by this feature. Both are stated here rather than
buried in planning notes, because both are places an overnight run can stop.

### 1. An unattended run can still stall at a Plan-stage checkpoint

Checkpoint auto-approval covers the Code stage and its fix loop. It does
**not** cover Plan. Since DevFlow's Define stage does no agent work, Plan is
the earliest stage where a run can stall — so unattended mode does not fully
deliver "complete a phase with nobody present".

**Cause.** The flag that would auto-approve a Plan-stage checkpoint is the same
flag that makes GSD's `plan-phase` chain straight into `execute-phase`. There
is no third setting for "bypass the checkpoint without chaining": upstream,
both behaviours read one boolean. Setting it at Plan would double-execute the
Code stage and misattribute its commits — worse than the stall it fixes.

**Fix owner: upstream.** Closing this needs a gsd-core change splitting that
boolean into separate bypass and chaining flags, tracked as **G-01**. It is not
a DevFlow fix. The operator considered making that upstream change a
prerequisite of this work and declined, so the limitation ships.

**What to do meanwhile.** If a phase's plan-stage work is likely to raise a
decision, plan the phase interactively first and then start the unattended run
— by the time `devflow start` reaches Plan with a plan already on disk, the
stage is a no-op.

### 2. A legacy-arm or non-Claude Code stage gets no chain-flag guard at all

There is no guard on those launch shapes, because the legacy arm is a detached
shell script with no process whose lifetime could bound the flag.

**Cause.** The guard is bound inside the pipe-owning monitor, and only the
Claude stream launch path starts one.

**Fix owner: DevFlow, and as of this release the failure is loud rather than
silent.** Condition 2 of the preflight now *refuses* this combination in auto
mode instead of letting the run start and stall later. That is an improvement
in legibility, not a removal of the limitation: `--mode auto` remains
Claude-only, and remains incompatible with the legacy launch opt-out.

### 3. GSD orchestrator may block auto-approval of `gate="blocking"` checkpoints

When a plan contains a `checkpoint:human-verify` task with `gate="blocking"`
(as opposed to `blocking-human`), GSD's own rules say it should auto-approve
in auto mode. In practice, the `execute-phase.md` orchestrator may inject a
conflicting instruction into the executor's prompt telling it to never
auto-approve — treating `gate="blocking"` as if it were the exception rather
than the auto-approvable default.

**Cause.** The GSD orchestrator (the parent agent running `execute-phase.md`)
generates the executor subagent prompt dynamically. When it sees
`checkpoint:human-verify` + `gate="blocking"`, it sometimes adds a
`<critical_gate>` block that overrides the executor's own checkpoint protocol.
This is an upstream GSD behavioral issue, not a DevFlow defect —
[open-gsd/gsd-core#3370](https://github.com/open-gsd/gsd-core/issues/3370).

**Practical impact is narrow.** GSD's default `human_verify_mode`
(`end-of-phase`, #3309) suppresses `checkpoint:human-verify` tasks from plans
entirely. A normal unattended run never contains these tasks and never hits
this. The limitation only affects phases whose CONTEXT.md locked decisions
or `human_verify_mode = mid-flight` setting force these checkpoints into plans.

**Fix owner: GSD.** The orchestrator's dispatch instructions should explicitly
state that `gate="blocking"` checkpoints are auto-approvable and the
executor's own protocol is authoritative.

## How this was verified

The mechanism was driven end-to-end against a real Claude agent running real
GSD commands on a throwaway fixture repository, in two arms differing by
exactly one flag — `--mode auto` against `--mode supervise` — so that a run
which merely never reached a checkpoint could not be mistaken for a successful
auto-approval. The run record, including both arms side by side and the
verbatim capture lines, is in
`.planning/phases/35.1-unattended-launch-prerequisites/35.1-DRILL.md`.

**What that drill does not establish:** it does not establish that DevFlow sets
and clears the flag at the right moments — that is covered separately by
DevFlow's own end-to-end tests, including a real-`SIGKILL` leak demonstration —
and one run is one sample. The drill also surfaced an upstream GSD behavioral
issue (#3370) where the orchestrator blocks auto-approval of correctly-authored
`gate="blocking"` checkpoints; this is recorded as known limitation #3 above.
