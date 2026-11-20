## CONFIRMED

### 1. MAJOR — Old server cleanup can delete the runner’s replacement failure gate

**Plan:** `.planning/phases/53-the-question-channel/53-07-PLAN.md:107` directs every non-answer ending to call “`Gates::close` for the stage.” `53-09-PLAN.md:98` independently closes abandoned questions in the runner before routing the stage outcome.

**Source:** `crates/devflow-core/src/gates.rs:1022` takes only root, phase and stage—no expected gate id—and removes that stage’s current gate at line 1118.

**Failure sequence:** the agent exits; the runner closes its question and opens a stage-failure gate; the old server then processes stdin EOF and calls its stage-wide close. The failure gate disappears while its runner is waiting for an answer. Both processes can also record different terminal reasons for the original question.

Require identity-checked cleanup under the gate lock. Control: delay server cleanup until the replacement failure gate exists and assert that replacement survives.

### 2. MAJOR — Abandoned ask-lock recovery allows two simultaneous owners

**Plan:** `53-07-PLAN.md:132`: “`Abandoned -> remove the file and retry`.”

**Source:** `crates/devflow-core/src/liveness.rs:209` probes through a temporary open; its result does not reserve the pathname. `Hold::take` publishes a newly locked inode through a hard link at line 150.

**Failure sequence:** two servers both probe an abandoned ask file; server A removes it and publishes its live replacement; server B executes its already-decided removal against A’s replacement, then publishes another lock. Both servers now own different locked inodes and can overwrite the same question gate.

The planned two-server test at `53-07-PLAN.md:128` tests contention against a live owner, which misses this recovery race. Use a permanent lock file or serialize reclamation, and test two contenders paused after observing the same abandoned file.

### 3. MAJOR — A stalled notify hook bypasses every promised timeout and cancellation path

**Plan:** `53-07-PLAN.md:77` calls the extracted notify helper before emitting `question_asked` and entering the answer wait. Line 107 puts cancellation checks inside that wait.

**Source:** `crates/devflow-core/src/gates.rs:1912–1925` executes the hook through blocking `Command::output()` with no timeout.

A hook that stalls leaves the worker blocked before it checks client cancellation, EOF, termination or the gate timeout. Even an operator answer cannot release it. This contradicts `53-07-PLAN.md:23` and its five-second cleanup acceptance criterion at line 114.

Add a bounded, cancellation-aware notification path. Control: hold the hook open, cancel the call, and assert timely gate removal and the intended close reason.

### 4. MAJOR — A successfully written operator abort can be discarded after a server crash

**Plan:** `53-09-PLAN.md:74` makes the server the sole writer of `abort_requested`. Line 98 tells the runner’s crash backstop to close the question before checking that field.

**Source:** `crates/devflow-cli/src/commands.rs:3413` publishes the abort answer independently. `crates/devflow-core/src/gates.rs:1173–1176` quarantines unclaimed answers during close; it does not apply an abort.

Kill the server after `gate abort` publishes its answer but before the server updates state. When the agent exits, the backstop disposes of the abort answer; `abort_requested` remains absent, so ordinary outcome routing continues. The operator’s abort becomes a failure/retry path instead of ending the run.

Recover a bound abort before disposing of the question. The acceptance test at `53-09-PLAN.md:71` covers only a surviving server; add this kill window.

### 5. MAJOR — Plan 10’s first task cannot pass its required build gate

**Plan:** `53-10-PLAN.md:71` deletes the checkpoint-declaration match arms, including the exhaustive `listed` match. Its tests must pass at lines 74–78. The corresponding `GateKind` variant is deleted only in Task 2, at line 95.

**Source:** `crates/devflow-cli/src/pipeline_outcomes.rs:565–580` exhaustively matches `Option<GateKind>`; `crates/devflow-cli/src/test_support.rs:761–775` exhaustively matches `GateKind`.

After Task 1, `GateKind::CheckpointDeclaration` still exists but those matches no longer cover it. Compilation fails before either verification test runs. Delete the variant and its exhaustive arms in one task, or retain explicit transitional arms until Task 2.

## SUSPECTED

### 6. MAJOR — Queued calls may exhaust their client timeout before receiving their own answer window

**Plan:** `53-07-PLAN.md:127–132` queues calls behind earlier questions; `53-01-PLAN.md:136` gives the worker an answer wait using the supplied timeout. `53-08-PLAN.md:28–29` sets each client timeout to only `T + 60`.

**Speculation:** if each worker wait starts a fresh T-second budget, a queued call spends the first question’s wait *plus* its own wait against a client deadline measured from submission. With the first question held longer than 60 seconds, the queued call cannot receive its full advertised answer window.

The plans specify no enqueue deadline. Confirm with two parallel calls and a short T: hold the first beyond the margin, then leave the second unanswered. Define whether queue time consumes T and test that policy explicitly.
