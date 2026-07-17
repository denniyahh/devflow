# Configuration

DevFlow has no project config file — there is no YAML/TOML file it reads on
startup, and no command that scaffolds or prints one. All workflow options
are supplied as flags to `devflow start`; runtime behavior that isn't a
per-run workflow choice is tuned via environment variables.

## `devflow start` flags

| Flag | Description |
|---|---|
| `--phase N` | Phase number to execute |
| `--agent claude\|codex\|opencode` | Agent to launch |
| `--mode auto\|supervise` | `auto` advances through Ship unattended (gating only at Ship, plus never-silent failure gates); `supervise` also gates every Validate for human review |
| `--force` | Overwrite the feature branch if it already exists |
| `--no-worktree` | Run directly in the primary checkout instead of an isolated worktree (worktree is the default) |
| `--dry-run` | Print the pipeline that would run without launching anything |

```bash
devflow start --phase 3 --agent claude --mode auto
```

## Environment variables

Runtime behavior — gate notifications, timeouts, and logging — is tuned via
environment variables, not config file fields:

| Variable | Default | Purpose |
|---|---|---|
| `DEVFLOW_GATE_NOTIFY_CMD` | unset | Shell command fired when a gate is written (e.g. an `ntfy.sh`/desktop-notification call) |
| `DEVFLOW_GATE_TIMEOUT_SECS` | 604800 (7 days) | How long a monitor waits at a gate before giving up |
| `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` | 120 | How long to wait on the shared-checkout lock before skipping the hook batch rather than running it unserialized |
| `RUST_LOG` | `info` | Log verbosity (stderr) |
| `DEVFLOW_LOG_FORMAT` | plain text | Set to `json` for machine-readable log lines |

Gate metadata (`DEVFLOW_GATE_PHASE`/`DEVFLOW_GATE_STAGE`/`DEVFLOW_GATE_CONTEXT`)
is passed as environment variables to the `DEVFLOW_GATE_NOTIFY_CMD` command,
never interpolated into the command string — the notify command itself is
still `sh -c`-evaluated, so treat it like any other shell command you control.

## Git-flow branch names

Branch naming (`main`, `develop`, `feature/` prefix) is hardcoded, not
configurable — DevFlow assumes a standard git-flow layout rather than
reading it from a config field.

## The full operator reference

This page only covers the "what can I configure" surface. For the complete
picture — the gate protocol, the notify hook, the `.devflow/` file
inventory, and a worked dogfood session — see
[OPERATIONS.md](../../OPERATIONS.md).
