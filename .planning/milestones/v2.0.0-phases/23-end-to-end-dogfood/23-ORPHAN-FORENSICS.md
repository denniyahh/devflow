# 23-ORPHAN-FORENSICS: why DevFlow leaks immortal monitors

Recorded 2026-07-25 by the execute-phase orchestrator, immediately before the
orphan population was destroyed at the operator's instruction. This document
exists because the processes themselves no longer do — it is the only surviving
record.

Companion to `23-PROBE-FINDINGS.md`. That document records what one run did.
This one records what **27 abandoned runs** have in common, and it changes the
justification for the supervisor work more than the probe alone did.

## Population at time of capture

| Metric | Value |
|---|---|
| `sh -c` monitor processes | 27 |
| `devflow advance` children | 27 |
| **Total processes** | **54** |
| Resident memory held | **168.6 MB** |
| Oldest gate | **30 hours** |
| Newest gate | minutes |

Every monitor had been reparented to PID 1965 — the process that launched each
one was long gone, but the monitor was not.

Scratch-dir classes:

- **24** — `--phase 12` (and one `--phase 08`) against empty `/tmp/.tmpXXXX`
  directories, from an earlier repro/test harness. These roots contain only
  `.devflow/`: no `.planning/`, no git repository, no source.
- **3** — real probe runs from phase 23 itself (`/tmp/devflow-probe-*`).

Binaries involved span `target/debug`, `target/release`, two now-removed
worktrees (`.worktrees/monitor-stop`, `.worktrees/phase-22`) and a session
scratchpad build (`repro/wt-test/target/debug`). Several of those build trees no
longer exist, yet their monitors were still running.

## Mechanism — confirmed, not inferred

The `sh -c` monitor body is:

```sh
apid=''; cleanup() { [ -n "$apid" ] && kill "$apid" 2>/dev/null; exit 0; }
trap cleanup TERM INT
cd '<root>' || exit 1
"$@" > '<root>/.devflow/phase-N-stdout' 2> '<root>/.devflow/phase-N-stderr.log' &
apid=$!
echo $apid > '<root>/.devflow/phase-N-agent-pid'
wait $apid
echo $? > '<root>/.devflow/phase-N-exit'
'<binary>' advance '<root>' --phase N
```

In **every** orphan, `wait $apid` had already returned — the agent was finished
and reaped, `phase-N-exit` was written. What remained running was the **final
line**: `devflow advance`.

Every one of the 27 corresponded to a scratch repo whose `state-NN.json` carried
`gate_pending: true` and whose `.devflow/gates/` held a gate file. Checked
explicitly against 7 dirs spanning every class and age band: **7 of 7 matched**,
and the remaining 20 all carried gate files.

**`devflow advance` blocks when the stage it evaluates raises a human gate.**

> **Correction — added 2026-07-25 after the phase-23 research re-aim.** The
> sentence that stood here read *"There is no timeout, no detach, no TTL, and no
> reaper,"* and its first clause was **wrong**. There *is* a timeout:
> `gate_timeout_secs()` defaults to **7 days**, overridable via
> `DEVFLOW_GATE_TIMEOUT_SECS` (`crates/devflow-cli/src/config_parse.rs:24-28`).
> The blocking wait is also a documented, deliberate design property rather than
> an oversight — `crates/devflow-core/src/lock.rs:8-9` states that `advance()`
> holds the per-phase lock *"across a gate's multi-day blocking wait."* No orphan
> in this population was older than 30 hours, so none was near expiry, which is
> why the measurement could not distinguish "7 days" from "forever."
>
> The finding survives; the framing sharpens. The defect is not an unbounded
> wait — it is a wait bounded so loosely it is **operationally indistinguishable
> from unbounded**. There is still no detach and no reaper, and 7 days is far
> past the point where an operator has stopped thinking about the run.

> Every DevFlow run that ends at a gate and is then abandoned holds a process
> pair resident for up to seven days.

## The inversion this proves

The phase's premise — and `OPERATOR-OBSERVABILITY-FINDINGS.md` Finding 1's
account of Phase 17 — is that the monitor **dies silently**.

The evidence says the opposite. Of 27 orphans, exactly **one**
(`.tmpUHVRlx`, phase 08) recorded anything resembling an agent death:

> `stage define failed: no work accounted for — agent process is gone with no
> commits and no declared external post-condition; human review needed`

and even that one did not die silently — it raised a gate and then waited 25
hours to be asked about it. The other 26 monitors were **working exactly as
designed** the entire time they were leaking.

Nearly every gate context carries an explicit `[never-silent]` marker:

```
[never-silent] stage code failed: Phase 12 has nothing to execute: project root
/tmp/.tmp0jvOBW contains only .devflow/ — no .planning/ directory, no git
repository, and no source tree. … — human review needed (retry, loop-to-code, or abort)
```

That marker is the fingerprint of a deliberate design decision: **never fail
silently — always raise a gate for a human.** It is a good decision. But it was
paired with a foreground `devflow advance` that waits on that gate with no
lifetime bound, and *that* pairing is what manufactures the orphans.

**The defect is not monitor death. It is monitor immortality.** DevFlow's
never-fail-silently guarantee has no expiry, so every unhappy path converts into
a process that outlives its operator's attention. 24 of the 27 were runs pointed
at empty directories — they failed instantly and correctly, then held a process
pair for up to 30 hours waiting for a human who was never coming.

## Two runs reached Ship, blocked by two different content gates

The orphan set contains a **second, independent** full-pipeline traverse that
`23-PROBE-FINDINGS.md` did not know about — the probe launched by this phase's
first retry executor, at `/tmp/devflow-probe-02-1785013744`:

```json
{"event":"advance_evaluated","stage":"validate","status":"success","verdict":"pass","ts":1785014577}
{"event":"transition","from":"validate","to":"ship","ts":1785014577}
{"event":"advance_evaluated","stage":"ship","status":"failed","reason":"review passed the
 Critical gate (0 critical, 5 warning, 4 info; REVIEW.md committed as a65c468), but
 /gsd-ship 1 blocked at preflight: (1) security ship gate active and blocking with
 workflow.security_enforcement=true and no SECURITY.md in the phase dir …"}
```

So within one hour, on the same machine and binary, **two separate runs reached
the Ship stage**:

| Run | Reached | Blocked by |
|---|---|---|
| `devflow-probe-02-1785013744` | ship | `/gsd-ship` preflight — `workflow.security_enforcement=true`, no SECURITY.md |
| `devflow-probe-23a-20260725-171553` | ship | review CR-01 — `01-VERIFICATION.md` false-green on an unrun Ship stage |

Neither was a process failure. Both were **content/config gates**, and they were
*different* content/config gates. This is materially stronger evidence for the
`23b INVALIDATED` verdict than the single run recorded in `23-PROBE-FINDINGS.md`:
the pipeline mechanism reaches Ship reliably, and what stops it is what it finds
when it gets there.

A third probe (`devflow-probe-23-02`, the first executor's) died at `define` with
`status: "ratelimited"` — `rate limited with no parseable retry time (usage
limit) — auto-resume cron not scheduled; resume manually`. That is the same
weekly-quota exhaustion that killed the executor driving it, and it is the
`infra_failures: 1` in that run's state. It is a real, separate finding: DevFlow
raises an unresolvable gate on rate-limit and gives no automatic resume.

## Corrections to the record

Two claims made earlier in this phase's execution were wrong, and are corrected
here rather than quietly dropped:

1. **"No probe process exists; the failed executors never launched a run."**
   False. Both failed executors launched real runs that were still alive at the
   time of the claim (`devflow-probe-23-02` and `devflow-probe-02-1785013744`).
   The detection command grepped for `devflow (advance|start|supervise)`, but at
   that moment the monitors had not yet reached their `devflow advance` line —
   the agent was still running — and the `sh -c` line quotes the binary path as
   `'<binary>' advance`, which the pattern did not match. The executors' claim
   that a run was live was **accurate**; the orchestrator's refutation was the
   error.

2. **"The probe died because its scratch repo was inside the ephemeral agent
   worktree and was reclaimed."** False. Both scratch repos were created under
   `/tmp`, survived worktree teardown intact, and are still on disk. The runs did
   not die — they **gated and hung**, which is this document's central finding.
   The real limitation remains that a subagent cannot own a run that outlives its
   turn, but the failure mode was orphaning, not destruction.

## What the replan should take from this

1. **Re-aim the supervisor work.** Its justification is not "the monitor dies."
   It is "the monitor never stops." A supervisor that owns the agent (D-10) is
   worth building if and only if it also bounds gate waits — a TTL, a detach, or
   a reaper. Otherwise it reproduces the leak with better logging.
2. **A gate needs a lifetime.** Any gate that can be raised unattended needs an
   expiry policy and a way to be enumerated across roots. There is currently no
   command that answers "what is gated on this machine?" — this document had to
   be assembled with `ps` and `find`.
3. **`devflow stop` (23d) is the missing primitive, and its absence is why this
   population exists.** Nothing could clean these up. They were removed with
   `kill(1)`.
4. **Rate-limit gates are unresolvable by construction** and should either
   auto-resume or self-expire; a gate whose remedy is "wait for a weekly quota
   reset" should not hold a process.
5. **Fix the false-green class first.** Both Ship blocks were content problems in
   the attestation layer (`VERIFICATION.md` scoring an unrun stage; a missing
   `SECURITY.md` under an enforcing gate), not mechanism problems. That is what
   currently stands between DevFlow and an unattended end-to-end run.

## Disposition

All 54 processes were terminated by the operator's explicit instruction
immediately after this document was committed. The `/tmp` scratch roots were left
in place; they are the raw evidence behind every quotation above and can be
re-read until the OS reclaims `/tmp`.
