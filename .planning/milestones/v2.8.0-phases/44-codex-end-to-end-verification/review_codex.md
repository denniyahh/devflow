## High — refused handoff still writes state

**Location:** `.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:24,157-170`; `crates/devflow-cli/src/pipeline_launch.rs:1252-1285,1155-1158`

**Defect:** The plan requires a refusal to leave the state file byte-identical and says the handoff pre-check must precede all mutation. However, the live `resume()` implementation clears `stopped`, applies the legacy-launch option, repairs flags, and unconditionally calls `workflow::save_state()` at line 1284 before `launch_stage()` invokes `run_preflight()`. A rejected target therefore occurs after a write.

**Consequence & reproduction:** Resume a stopped phase with `--agent` targeting an unsafe Define/Plan driver. The command returns through preflight, but the persisted state has already changed (`stopped`, stop fields, repair fields, serialization/timestamps/format), violating D-08 and the plan’s byte-identical acceptance criterion.

---

## High — “exactly two fields change” is incompatible with existing resume behavior

**Location:** `.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:25,170-178`; `crates/devflow-cli/src/pipeline_launch.rs:1252-1264,1278-1285`

**Defect:** The plan claims a handoff changes only `agent` and `monitor_pid`, while explicitly leaving existing resume logic untouched. Existing logic mutates `stopped`, `stop_reason`, `stop_until`, `legacy_claude_launch`, and possibly auto-chain repair state before launch. `spawn_agent_and_record()` also writes `monitor_pid = None` before validating/spawning.

**Consequence & reproduction:** A handoff from a stopped or legacy-opted-out state produces more than the two claimed field changes. The proposed “preserves every State field except agent and monitor_pid” test cannot pass against the actual control flow unless it suppresses legitimate mutations, making the test contract internally contradictory.

---

## High — cron deletion misses legacy records while claiming D-17/D-18 coverage

**Location:** `.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:185-193`; `44-02-PLAN.md:197-203`; `crates/devflow-core/src/ship.rs:139-152`

**Defect:** Both plans require checking only `cron_instructions_path(...phase).exists()` and emitting `path_kind: "per-phase"` only when that check succeeds. But `delete_cron_instructions()` also deletes the legacy `.devflow/cron-instructions.json` record. The plans explicitly decline to inspect that path.

**Consequence & reproduction:** Create only a legacy record naming phase 7, then successfully resume or ship phase 7. The pre-check is false, so the planned deletion/event path is skipped; the legacy record remains and D-17 cleanup is not achieved. Even if deletion is called unconditionally, the emitted audit event would falsely omit `path_kind: "legacy"`.

---

## High — no atomicity between state persistence, audit emission, and process spawn

**Location:** `.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:170-183`; `crates/devflow-core/src/workflow.rs:174-192`; `crates/devflow-core/src/events.rs:34-70`

**Defect:** The proposed sequence is `save_state` → `events::emit` → spawn. State writes are atomic, but event emission is fail-soft and can silently return on I/O failure; there is no transaction or recovery marker spanning the three operations.

**Consequence & reproduction:** Kill the process after the state rename but before event append, or make `events.jsonl` unwritable. The persisted state says the new agent is active, but no `agent_handoff` audit exists. Conversely, a monitor can be spawned after an audit write but before the monitor PID save. The design cannot guarantee the claimed audit/state/process correspondence.

---

## Medium — verification commands are syntactically invalid for multiple test names

**Location:** `.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:216`; `44-04-PLAN.md:337`

**Defect:** Each command passes five test names as separate positional arguments to `cargo test`. Cargo accepts one test-name filter; additional names are not independent filters.

**Consequence & reproduction:** Running the exact command yields Cargo argument parsing failure (or does not select the intended tests), so the claimed “5 passed” parity check is never performed. The plan needs separate invocations, a single module filter plus assertions, or a dedicated test target.

---

## Medium — piped `cargo test | rg` checks can report success for incomplete/failed runs

**Location:** `.planning/phases/44-codex-end-to-end-verification/44-01-PLAN.md:214-216`; `44-03-PLAN.md:185-188`; `44-04-PLAN.md:335-337`

**Defect:** The verification commands rely on a pipeline whose exit status is `rg`, not Cargo. They search for a matching “test result: ok”/“5 passed” line rather than asserting Cargo’s exit status and exact selected-test count. A partial test run can contain an earlier successful result line while another target fails. The workspace failure-count pipeline is also brittle: when no `FAILED` lines exist, the first `rg -c` exits nonzero and the second stage receives no `0`.

**Consequence & reproduction:** A multi-target Cargo invocation with one passing target and one failing target can still satisfy the grep expression. A clean workspace run can make the “zero FAILED lines” pipeline itself exit failure. These checks do not reliably verify the stated conditions.

---

## Medium — malformed calendar dates are not fail-closed

**Location:** `crates/devflow-core/src/ship.rs:255-297,350-357`; `.planning/phases/44-codex-end-to-end-verification/44-03-PLAN.md:113-124,178-180`

**Defect:** `parse_rfc3339ish()` only bounds `day` to `1..=31`; `days_from_civil()` normalizes invalid dates instead of rejecting them. The plan strengthens tests only for `"unknown"` and schedule emptiness, not invalid calendar values.

**Consequence & reproduction:** An agent emits `2026-02-31T15:45:00Z`. The parser returns `Some`, converts it to a different valid date, and schedules that instant rather than failing closed. This violates the stated unparseable-input safety guarantee.

---

## Medium — the live-dogfood “checkpoint” is procedural, not enforced

**Location:** `.planning/phases/44-codex-end-to-end-verification/44-04-PLAN.md:193-204,241-253`

**Defect:** Task 2 says to stop and wait for an operator response, but the plan provides no executable gate, persisted approval token, or tooling check preventing an executor from continuing. The acceptance criteria only inspect operator-written text, file timestamps, and captures.

**Consequence & reproduction:** An executor can select an option, launch Codex, or fabricate a “run complete” response without an independent mechanism rejecting that sequence. Modification times can also be changed after the fact; they do not prove a process produced the files. This is insufficient for the plan’s claimed non-fakable human checkpoint and evidence provenance.

---

## Medium — evidence requirements do not prove “raw captured output”

**Location:** `.planning/phases/44-codex-end-to-end-verification/44-04-PLAN.md:211-215,247-252,340-353`

**Defect:** The plan forbids hand-authored evidence but verifies only file existence, text content, timestamps, and self-reported attempt counts. There is no process-generated manifest, hash chain, append-only capture, or independent command transcript tying files to the Codex PID.

**Consequence & reproduction:** A plausible output file can be created after a failed or absent run and given a timestamp inside the recorded window. Task 3’s gap disposition can then treat it as ground truth, so CODE-01 may be marked satisfied without proving a Codex process executed the phase.