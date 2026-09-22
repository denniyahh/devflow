## CONFIRMED

### C-1. Temp-only state-write artifacts are still invisible to `recover --clean`
**File:** `snapshot/crates/devflow-core/src/workflow.rs:186`, `snapshot/crates/devflow-core/src/workflow.rs:291`, `snapshot/crates/devflow-core/src/recover.rs:125`  
**Mechanism:** State writes create `.state-NN.json.<pid>.<seq>.tmp`. `state_file_phases` recognizes only `state-NN.json`; a temp-only phase never enters the stale-state loop, and gates cannot discover it. `clear_state` would remove it, but is never called.  
**Reproduction:** Create `.devflow/.state-48.json.1.0.tmp` with no `state-48.json`, then run `recover --clean`. The temp remains; the CLI reaches `no stale workflow state was cleaned`. `--phase 48` does reach it.  
**Severity:** medium — a sanctioned all-phase reset leaks a known state-write artifact.

### C-2. State-temp deletion failures are logged but reported as successful cleanup
**File:** `snapshot/crates/devflow-core/src/workflow.rs:357`, `snapshot/crates/devflow-core/src/recover.rs:194`, `snapshot/crates/devflow-cli/src/commands.rs:2609`  
**Mechanism:** `clear_state` catches every orphan-temp removal error, emits only a tracing warning, and still returns `Ok(true)` if it removed the state. The sweep adds the phase to `cleared`; `recover_cmd` prints success and exits zero. The explicit `--phase` path has the same behavior.  
**Reproduction:** For a stale `state-48.json`, create a directory at `.state-48.json.1.0.tmp`. Cleanup removes state but cannot unlink the directory; the artifact remains while the command reports cleanup succeeded.  
**Severity:** medium — directly defeats WR-04’s nonzero/reporting contract for a listed artifact class.

### C-3. Corrupt cron records are unreachable by the all-phase sweep
**File:** `snapshot/crates/devflow-core/src/ship.rs:127`, `snapshot/crates/devflow-core/src/recover.rs:151`, `snapshot/crates/devflow-cli/src/commands.rs:2621`  
**Mechanism:** `list_cron_instructions` silently skips unparsable `cron-instructions*.json` records. The all-phase sweep only calls deletion for records returned by that parser, so corrupt per-phase and legacy records survive indefinitely.  
**Reproduction:** Place invalid JSON in `.devflow/cron-instructions-48.json` or legacy `cron-instructions.json`, with no state or gates. `recover --clean` reports no stale state cleaned and leaves the record. `recover --clean --phase 48` does delete it.  
**Severity:** medium — `recover --clean` does not reset every cron artifact it claims to cover.

### C-4. A valid orphan cron record is deleted but omitted from every success report
**File:** `snapshot/crates/devflow-core/src/recover.rs:151`, `snapshot/crates/devflow-core/src/ship.rs:153`, `snapshot/crates/devflow-cli/src/commands.rs:2609`  
**Mechanism:** The all-phase cron sweep ignores `delete_cron_instructions`’s new `bool`. With only a valid cron record present, neither `cleared` nor `orphan_gates_cleared` is populated, so the CLI says `no stale workflow state was cleaned` after deleting the record.  
**Reproduction:** Write a valid phase-48 cron record without state/gates and run `recover --clean`; it is removed but no output identifies that removal.  
**Severity:** low — no residual artifact, but WR-03’s “report only what actually happened” contract is still incomplete.

### C-5. Stale-lock removal failures do not affect `recover`’s exit status
**File:** `snapshot/crates/devflow-core/src/lock.rs:449`, `snapshot/crates/devflow-core/src/recover.rs:146`, `snapshot/crates/devflow-cli/src/commands.rs:2624`  
**Mechanism:** `remove_stale_locks` returns indistinguishable warning strings for skipped and failed removals. Both clean-report functions append them without setting `removal_failed`; `recover_cmd` exits nonzero only from that flag.  
**Reproduction:** Make a stale readable `lock-48`, pre-create its coordination inode, then deny directory deletion permission after it is readable. Lock cleanup warns that it could not remove the lock, yet `recover --clean` exits zero.  
**Severity:** medium — a stale lock can remain and wedge later work despite a nominally successful reset.

### C-6. `phases_on_disk` treats arbitrary gate-directory names as phases
**File:** `snapshot/crates/devflow-core/src/gates.rs:192`, `snapshot/crates/devflow-core/src/recover.rs:206`, `snapshot/crates/devflow-core/src/lock.rs:187`  
**Mechanism:** Any UTF-8 gate-directory name beginning with `48-` is accepted as phase 48. A lone `gates/48-not-a-gate` causes the orphan sweep to acquire phase 48’s lock and create its permanent coordination inode, though no gate artifact exists. Canonical cleanup does not delete the planted file.  
**Reproduction:** Create `.devflow/gates/48-not-a-gate` without state, then run `recover --clean`. It performs an unnecessary lock cycle and leaves the non-gate file.  
**Severity:** low — no canonical gate is deleted, but the parser is broader than its documented protocol.

## SUSPECTED

None. The remaining live-gate race is the explicitly deferred WR-05/999.136 case, so I did not restate it.

## CHECKED AND CLEAN

- Legacy monitor post-reap window: `reaped=1` gates both marker creation and `kill`; a TERM during the tail cannot affect the next agent. `snapshot/crates/devflow-core/src/monitor.rs:501`
- The narrow pre-`reaped=1` TERM window exits before the advance tail, leaving only a stale marker that the next Legacy launch removes. `snapshot/crates/devflow-core/src/monitor.rs:507`
- Bash behavior: the added monitor regression test passed exactly once; removing `reaped=1` made it fail with the marker assertion. `snapshot/crates/devflow-core/src/monitor.rs:3328`
- Bash `wait` interruption: a direct trapped-TERM probe ran the trap and did not execute code after `wait`; this supports the comment’s ownership assumption for bash only. Dash was unavailable locally, so its CI behavior was not re-executed.
- `MonitorTail::Script` is test-only under `#[cfg(test)]`; production always selects `Advance`. `snapshot/crates/devflow-core/src/monitor.rs:306`
- The orphan-gate regression test passed exactly once; deleting the orphan sweep made it fail with `0 passed; 1 failed`. `snapshot/crates/devflow-core/src/recover.rs:966`
- Canonical request/response/ack files and their generated `.tmp` forms are reached by both all-phase orphan sweeping and explicit `--phase` cleanup. `snapshot/crates/devflow-core/src/gates.rs:322`
- Generated gate names, including hidden temp names, parse through `phases_on_disk`; no generated gate filename is missed. `snapshot/crates/devflow-core/src/gates.rs:195`
- Parsable `state-NN.json` is swept only when stale and lock-free; explicit `--phase` reaches it regardless of staleness. `snapshot/crates/devflow-core/src/recover.rs:165`
- A corrupt per-phase state is deliberately retained and warned, rather than being mistaken for an orphan gate. `snapshot/crates/devflow-core/src/recover.rs:131`
- Corrupt legacy `state.json` is removed and explicitly reported by the all-phase sweep; explicit phase cleanup correctly does not guess which phase it belonged to. `snapshot/crates/devflow-core/src/workflow.rs:318`
- Valid per-phase and matching legacy cron records are reached by `--phase`; only the all-phase reporting and corrupt-record paths are defective. `snapshot/crates/devflow-core/src/recover.rs:337`
- Agent stdout/exit files are intentionally archived before relaunch, agent PID and idle-timeout records are cleared there, and stderr/stop-marker/prompt are replaced or cleared by their respective launch paths. `snapshot/crates/devflow-core/src/agent_result.rs:3164`
- Per-phase capture history and project-wide `events.jsonl` are retained intentionally as diagnostic/audit evidence, not reset artifacts. `snapshot/crates/devflow-core/src/agent_result.rs:3128`, `snapshot/OPERATIONS.md:137`
- Lock coordination inodes are intentionally persistent; only their temporary writer leftovers lack a sweep. `snapshot/crates/devflow-core/src/lock.rs:187`
- Changed `Gates::cleanup` callers preserve their prior error behavior; ordinary callers discard the new boolean, while recovery is the intended consumer. `snapshot/crates/devflow-cli/src/pipeline_launch.rs:1167`
- Changed `clear_state` callers likewise preserve prior behavior except the recovery report paths that now consume its boolean. `snapshot/crates/devflow-cli/src/pipeline_gate.rs:275`
- `delete_cron_instructions` is consumed correctly by explicit phase cleanup; the all-phase caller is the missed sibling. `snapshot/crates/devflow-core/src/recover.rs:337`


