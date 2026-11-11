# Phase 52: Run Notes

Per the dogfooding rule in CLAUDE.md, every stage or step run outside `devflow start` is recorded here with its reason.

## Criterion 1 control: the installed 3.0.0 binary's stage list (2026-11-10)

Recorded before anything in this phase rebuilds or reinstalls the binary (D-18, ROADMAP criterion 1), so a later
dry run from the 4.0 binary can be compared with what 3.0.0 printed.

- **Date:** 2026-11-10
- **Binary:** `~/.cargo/bin/devflow` (installed release, not a build from this worktree)
- **Version:** `~/.cargo/bin/devflow --version` printed:

  ```text
  devflow 3.0.0
  ```

- **Where it ran:** a scratch clone of the phase worktree (`git clone --local --shared`, branch `feature/phase-52` at
  `3446749`) under the session scratchpad. The clone had no changes after either run (`git status --short --ignored`
  printed nothing), and it has been deleted.

### Run 1: the plan's command, as written. Refused by clap

```text
~/.cargo/bin/devflow start --dry-run --phase 52
```

Exit code 2, stdout empty. stderr, verbatim:

```text
error: the following required arguments were not provided:
  --mode <MODE>

Usage: devflow start --phase <PHASE> --mode <MODE> --dry-run [PROJECT]

For more information, try '--help'.
```

Reason: 3.0.0's `start` requires `--mode`, even with `--dry-run`. The plan's command left it out.

### Run 2: the same command with the one argument clap asked for

`--mode supervise` is the mode the project's run conventions use. Nothing else was changed.

```text
~/.cargo/bin/devflow start --dry-run --phase 52 --mode supervise
```

Exit code 0, stderr empty. stdout, verbatim:

```text
dry run — phase 52 | agent claude | mode supervise

stage pipeline:
  define /gsd-discuss-phase 52
  plan /gsd-plan-phase 52
  code /gsd-execute-phase 52
  validate /gsd-validate-phase 52 [GATE]
            ↳ hooks: [DocsUpdate]
  ship /gsd-ship 52 [GATE]

ship gate: not pre-authorized

after ship: [Merge, VersionBump, ChangelogAppend, BranchCleanup]
```

**Define and Ship:** Define is the first stage line (`define /gsd-discuss-phase 52`) and Ship is the last
(`ship /gsd-ship 52 [GATE]`). The after-ship hooks follow on the `after ship:` line.

**What this does not establish:** this is one dry run in `supervise` mode. It shows what 3.0.0 *prints* as its
pipeline. It does not show that 3.0.0 *runs* those stages: no stage was launched.

## Exception: not dogfooded (D-18)

Phase 52 runs with GSD commands in an agent session (`/gsd-plan-phase 52`, `/gsd-execute-phase 52`, ...), not with
`devflow start`. D-18 in `52-CONTEXT.md`, verbatim:

> **D-18:** Phase 52 is **not dogfooded**. It runs with GSD commands in an agent session (`/gsd-plan-phase 52`,
> `/gsd-execute-phase 52`, …), not with `devflow start`. This is an exception to the CLAUDE.md dogfooding rule, which
> says DevFlow drives its own phases from Phase 51 on, and is recorded here as that rule requires. The criterion 1
> control (a `--dry-run` of the installed 3.0.0 binary) is a dry run, not a dogfood run, and still applies.

## Close-out: no release (operator, 2026-11-11)

The operator chose not to cut 4.0.0 at Phase 52 close-out, because DevFlow is not yet fully usable. Phase 52 merges
to `develop` with the workspace version set to `4.0.0-alpha.1` (no tag, no crates.io publish, nothing to `main`), so
a build from `develop` never reports the published 3.0.0. 4.0.0 is cut at the v4.0 milestone close.

- Backlog 999.226 (devflow-core `pub` items with no caller) was taken here on the operator's answer: removed before any
  4.x publish.
- Backlog 999.220 (`scripts/cut-release.sh` cannot see `BREAKING CHANGE` footers) is not needed until a release is
  cut; it moves to the v4.0 milestone close, before the 4.0.0 cut.
- The close-out itself is done by hand with `scripts/cut-pr-branch.sh` from `feature/phase-52`, per the run
  conventions in CLAUDE.md; that is the convention, not a further exception.
