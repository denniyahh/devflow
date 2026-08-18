# Pi Subagent Dispatch

Pi has no built-in subagent tool. Subagent dispatch is an **optional, user-owned
extension** (`@bacnh85/pi-subagent`) that DevFlow detects and reports — it is
never shipped, managed, or routed by DevFlow.

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
probing `pi list --no-approve` and matching the vetted `@bacnh85/pi-subagent`
package (not a bare `*subagent*` name-match — unsafe/deferred packages like
`@mystilleef`/`@dreki-gg`/`@smoose` are not reported available).

- **Present** → the capability is reported `available`.
- **Absent / undetectable** → `not installed` (a warning; Pi runs single-agent,
  always valid). Detection fails **closed to baseline**, never refuses a
  working run.

**Honest limit:** the signal is *name-based* (Pi exposes no `pi tools` command);
it does not prove the extension registers a working dispatch tool. It is
observable and testable (`pi list` output), not inferred.

`devflow doctor` surfaces the result as `pi subagent dispatch` —
`available` (✓) or `not installed` (⚠, with the install hint).

## Routing

There is no routing. The launch argv, prompt, and supervision are identical
whether or not a subagent extension is installed (`pi -p --no-approve
"<prompt>"`). `subagent_dispatch` is **reported only** — `devflow doctor` is
its sole consumer; no `start`/`pipeline_launch`/`prompt`/`advance` logic
branches on it. It is the hook any *future* stage-routing logic could key on.
