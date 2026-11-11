# Configuration

DevFlow requires no initialization step. Workflow options are supplied as
flags to `devflow start`; an optional minimal `devflow.toml` controls only
reliability knobs.

```toml
capture_retention = 5
external_verify_enabled = true
fallback_agent = "antigravity"   # optional: the rate-limit fallback for the Code stage
```

`fallback_agent` is armed by `devflow start` only, as the run's one-hop
Code-stage fallback when `--fallback-agent` is not given; the flag overrides
its value, `devflow resume` does not read it, and no environment variable sets
it. A key that names no agent, or a `devflow.toml` that does not
parse, still refuses `devflow start` even when `--fallback-agent` is given. See
OPERATIONS.md's rate-limit failover section for what the fallback does.

## `devflow start` flags

| Flag | Description |
|---|---|
| `--phase N` | Phase number to execute |
| `--agent claude\|codex\|opencode\|pi` | Agent to launch |
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
| `DEVFLOW_GATE_NOTIFY_CMD` | unset | Shell command fired when a gate is written (e.g. an `ntfy.sh`/desktop-notification call). Saved per run by `start`/`resume`/`advance`; empty turns it off for the run (see OPERATIONS.md) |
| `DEVFLOW_GATE_TIMEOUT_SECS` | 259200 (3 days) | How long a supervisor waits at a gate, registered as its waiter and holding no lock. Not a deadline for answering — the gate file never expires. Past the timeout the run ends at that gate, and `devflow resume` reruns the stage and asks again, setting any unused answer aside |
| `DEVFLOW_GATE_MAX_UNATTENDED_AGE_SECS` | 259200 (3 days) | Age threshold `devflow gate sweep` uses to call a gate abandoned. Held equal to `DEVFLOW_GATE_TIMEOUT_SECS`, never shorter |
| `DEVFLOW_CAPTURE_RETENTION` | 5 | Override capture generations retained per phase |
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
