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

## Decision triage (`[triage]`)

Decision triage asks TypeSafe's Jev model whether a question an agent asks
can be answered the way you answered earlier ones, and answers it when both
scores reach their thresholds; every other question waits for you as before.
It is off unless `devflow.toml` turns it on:

```toml
[triage]
enabled = true
threshold = 0.9            # least probability of the chosen option that decides
pre_check_threshold = 0.7  # least probability that you would take an offered option at all
model = "jev-1.14.0"
timeout_secs = 10
exclude_label_phases = [51, "52.1"]
```

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `false` | Whether triage runs. No `[triage]` table, or no `enabled` key, means off |
| `threshold` | unset | A question is decided only when the chosen option's probability is at least this value |
| `pre_check_threshold` | unset | ...and the probability that you would accept an offered option as offered is at least this value |
| `model` | `jev-1.14.0` | The versioned model id sent; a response from any other model is not used |
| `timeout_secs` | 10 | How long one triage request may take |
| `exclude_label_phases` | empty | Phases whose recorded answers are not used as examples of how you answer |

Both comparisons are "at least": a score equal to its threshold decides. An
unset threshold escalates every question, and both are unset until a measured
value ships, so turning triage on without thresholds changes nothing.
A `blocking-human` question always goes to you, whatever the thresholds.

Triage fails safe. A threshold that is not a number in [0, 1], a `model` that
is not a non-empty string, or an `exclude_label_phases` entry that is not a
phase id makes triage escalate every question; an `enabled` that is not
`true` or `false`, or a `devflow.toml` that does not parse, turns triage off;
a bad `timeout_secs` uses the default. Each case writes a `warning:` line
naming the key. An unknown key in `[triage]` is ignored with a warning.

Example answers are read only from this file's exclusions. In the edit that
sets `enabled = true`, also list in `exclude_label_phases` every phase whose
recorded answers were not yours (answered by an agent or a default), so they
are never used as evidence of how you answer.

## `devflow start` flags

| Flag | Description |
|---|---|
| `--phase N` | Phase number to execute |
| `--agent claude\|codex\|opencode\|pi` | Agent to launch |
| `--mode auto\|supervise` | `auto` runs unattended and finishes on a passing Validate (gating only on repeated Validate failure, plus never-silent failure gates); `supervise` also gates every Validate for human review |
| `--no-worktree` | Run directly in the primary checkout instead of an isolated worktree (worktree is the default) |
| `--dry-run` | Check, read-only, what a real start would refuse, then print the pipeline that would run; launches and writes nothing and names the start checks it skips |

`start` removes nothing: over a leftover phase worktree, branch or run state it
refuses, naming `devflow discard --phase N`, which lists what it removes first and
refuses a branch with commits not merged into the base unless you name it; the
refusal over an incomplete run also names `devflow resume --phase N`.

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
| `RUST_LOG` | `off` | Tracing verbosity (stderr). Unset or empty prints no tracing lines; set a level such as `info` or `debug` to see them |
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
