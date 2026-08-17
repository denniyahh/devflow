# Pi Subagent Dispatch

Pi has no built-in subagent tool. Subagent dispatch is an **optional, user-owned
extension** (`@bacnh85/pi-subagent`) that DevFlow detects and routes around — it
is never shipped or managed by DevFlow.

## Install (user-owned)

The operator installs the extension at **user scope** (the trust boundary
`--no-approve` depends on):

```bash
pi install npm:@bacnh85/pi-subagent
```

This makes a `subagent` tool available to Pi sessions. The extension is
synchronous and in-process: its `execute()` awaits its children, so the parent
`pi -p --no-approve` process stays alive until subagents finish, then emits the
`DEVFLOW_RESULT` marker and exits. `MonitorLaunch::Legacy` process-exit
supervision observes it — **no drain gate, no `PipeOwning`**.

## Capability detection

`PiDriver::capabilities()` reports `DriverCapabilities::subagent_dispatch` by
probing `pi list --no-approve` and matching a package name containing `subagent`.

- **Present** → the **dispatch arm**: the run may delegate to subagents.
- **Absent / undetectable** → the **baseline arm**: Pi runs single-agent (always
  valid). Detection fails **closed to baseline**, never refuses a working run.

**Honest limit:** the signal is *name-based* (Pi exposes no `pi tools` command);
it does not prove the extension registers a working dispatch tool. It is
observable and testable (`pi list` output), not inferred.

`devflow doctor` surfaces the result as `pi subagent dispatch` —
`available` (✓) or `not installed` (⚠, with the install hint).

## Routing

The launch argv is identical in both arms (`pi -p --no-approve "<prompt>"`). The
capability decides what DevFlow *expects and advertises* — subagent delegation
is available to the run only when a subagent extension is installed. The
capability is the hook any stage-routing logic keys on.
