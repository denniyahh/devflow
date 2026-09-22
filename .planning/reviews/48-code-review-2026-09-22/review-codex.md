## CONFIRMED

### C-1. Implicit `recover --clean` deletes a live, gate-waiting phase

**File:** `crates/devflow-cli/src/commands.rs:2552`; `crates/devflow-core/src/recover.rs:93`, `:109`, `:173`, `:196`; `crates/devflow-cli/src/pipeline_launch.rs:1621`; `crates/devflow-cli/src/pipeline_gate.rs:369`; `crates/devflow-core/src/workflow.rs:309`

**Mechanism:** The implicit form (`recover --clean`, no `--phase`) calls `recover::clean`. It classifies staleness from `started_at` plus the exited agent child PID, never acquires or checks the phase lock, then deletes state. A monitor in `advance_with` holds that lock while `run_gate_with_timeout` waits; after 24h, its agent PID is normally gone even though the monitor remains live.

**Reproduction:** In a disposable copy, I added a test that creates a stale state, holds its real phase lock, then calls `recover::clean`.  
`cargo test -p devflow-core recover::tests::proof_clean_deletes_a_stale_state_while_its_phase_lock_is_live -- --exact`  
Output: `running 1 test` → `1 passed`. The state was deleted while the live lock remained. Controls confirmed that a live agent PID is retained and an unlocked stale state is deleted.

**Severity:** high — an automatic recovery command deletes state and cron recovery data for an active waiter. If that waiter then dies before another state write, the phase is no longer recoverable from persisted state.

### C-2. Gate responses are not bound to a gate incarnation

**File:** `crates/devflow-cli/src/commands.rs:1414`, `:1430`; `crates/devflow-core/src/gates.rs:195`, `:202`, `:242`, `:263`; `crates/devflow-cli/src/pipeline_gate.rs:369`; `crates/devflow-cli/src/pipeline_launch.rs:1503`

**Mechanism:** `gate approve|reject` checks a phase-level holder before calling `Gates::respond`; `respond` separately checks that the request and response paths exist or do not exist, then later hard-links its prepared response. It never holds the phase lock and carries no generation token.

A responder can pass both checks, a restarted/cleaning process can remove the old gate, then the responder can hard-link the old response after cleanup. A later same-phase/same-stage `write_gate` replaces only the request; `poll_response` immediately consumes the surviving old response.

Separately, `resume` relaunches the saved stage without removing a prior answered response. Thus a later failure at that same stage can consume an answer intended for the earlier run.

**Reproduction:** In a disposable copy, I recreated the source-permitted final filesystem state: old response written after cleanup, then a fresh same-stage request.  
`cargo test -p devflow-core --test proof_stale_response`  
Output: `running 2 tests` → `2 passed`: the late old approval is consumed immediately; the negative control with no late response remains unanswered. This is not a scheduler-stress proof, but the source ordering above permits that exact interleaving.

**Severity:** high — an old approval can advance or retry a different gate; an old rejection containing `abort` can abort a later run without a new human decision.

## SUSPECTED

None.

## CHECKED AND CLEAN

- Lock publication/reclaim serializes DevFlow lock-path mutation through the separate coordination inode: `crates/devflow-core/src/lock.rs:194`, `:208`, `:243`, `:409`.
- Competing writers to one still-current response pathname are first-writer-wins via hard link: `crates/devflow-core/src/gates.rs:404`.
- Checkpoint parsing, preflight recording, and resume re-scan use the shared declaration representation: `crates/devflow-core/src/verify.rs:179`, `:319`; `crates/devflow-cli/src/preflight.rs:1315`; `crates/devflow-cli/src/pipeline_launch.rs:1795`.
- Child-PATH tests reject unmatched zero-test execution rather than accepting a vacuous green result: `crates/devflow-core/src/test_support.rs:154`, `:183`.


