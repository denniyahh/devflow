# Configuration

DevFlow requires no initialization step. Workflow options are supplied as
flags to `devflow start`; an optional minimal `devflow.toml` controls only
reliability knobs.

```toml
capture_retention = 5
review_angles = ["doc accuracy", "security", "CI correctness", "external state"]
external_verify_enabled = true
```

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
| `DEVFLOW_FOREGROUND_GATE_TIMEOUT_SECS` | 60 | How long `devflow ship --phase`'s foreground manual override waits for a reopened Ship gate before failing fast, instead of `DEVFLOW_GATE_TIMEOUT_SECS`'s multi-day default |
| `DEVFLOW_CHECKOUT_LOCK_TIMEOUT_SECS` | 120 | How long to wait on the shared-checkout lock before skipping the hook batch rather than running it unserialized |
| `DEVFLOW_CAPTURE_RETENTION` | 5 | Override capture generations retained per phase |
| `DEVFLOW_REVIEW_ANGLES` | built-in list | Override Ship review angles with a comma-separated list |
| `DEVFLOW_EXTERNAL_VERIFY_ENABLED` | true | Enable or disable PLAN-declared external probes |
| `DEVFLOW_TRUST_EXTERNAL_VERIFY` | unset | Exact reviewed PLAN probe commands as a JSON string array; required in addition to enabling probes |
| `RUST_LOG` | `info` | Log verbosity (stderr) |
| `DEVFLOW_LOG_FORMAT` | plain text | Set to `json` for machine-readable log lines |

Gate metadata (`DEVFLOW_GATE_PHASE`/`DEVFLOW_GATE_STAGE`/`DEVFLOW_GATE_CONTEXT`)
is passed as environment variables to the `DEVFLOW_GATE_NOTIFY_CMD` command,
never interpolated into the command string — the notify command itself is
still `sh -c`-evaluated, so treat it like any other shell command you control.

PLAN files can be written by an agent. DevFlow therefore never executes an
`external_verify` command unless the parent DevFlow process receives the exact
reviewed commands, for example
`DEVFLOW_TRUST_EXTERNAL_VERIFY='["test -f shipped.txt"]'`. If PLAN command
bytes change after review, evaluation fails closed without executing them.

## Git-flow branch names

Branch naming (`main`, `develop`, `feature/` prefix) is hardcoded, not
configurable — DevFlow assumes a standard git-flow layout rather than
reading it from a config field.

## The full operator reference

This page only covers the "what can I configure" surface. For the complete
picture — the gate protocol, the notify hook, the `.devflow/` file
inventory, and a worked dogfood session — see
[operator reference](https://github.com/denniyahh/devflow/blob/develop/OPERATIONS.md).
