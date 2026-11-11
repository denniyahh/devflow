## CONFIRMED

Static review only. No tests or live runs executed; no files edited or temporary artifacts created.

1. **Major — D-06’s rename does not fix its stated `ETXTBSY` mechanism.**

   `.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:51`: “write the stub to a temp name … close it, then `rename` it into place.”

   The backlog’s justification is false: `.planning/ROADMAP.md:848–849` claims “the exec’d inode was never open for writing.” Rename preserves the inode. Under the hypothesis at ROADMAP lines 844–845, another child still owns a writable descriptor to that inode after the parent closes and renames it. Executing the renamed script can still produce `ETXTBSY`.

   The current writer is `crates/devflow-core/src/test_support.rs:414–416`. Require a controlled reproduction that retains the inherited writable descriptor through publication, then closes it. Atomic publication alone cannot establish that this race is fixed.

2. **Major — D-09 authorizes precisely the deletion D-10 forbids.**

   `.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:65–66` permits discard of an unmerged branch when “the operator names the branch.”

   Lines 73–75 then require: “Every new path must refuse to delete an unmerged branch.” The explicit-discard path is a new path. For an unmerged branch explicitly named by the operator, these decisions require opposite outcomes.

   `.planning/ROADMAP.md:801–802` includes the explicit-name exception. D-10 needs that exception stated, including whether naming the branch confirms its identity or selects an arbitrary deletion target. Otherwise implementation and acceptance tests cannot both satisfy the context.

3. **Major — D-05 investigates the wrong descriptor boundary.**

   `.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:47` asks which descriptors lack `O_CLOEXEC` and which helpers “fork without exec.”

   The suspected race requires neither condition. Close-on-exec descriptors remain inherited **before** exec. `.planning/ROADMAP.md:861–863` explicitly describes that window, including an `O_CLOEXEC` descriptor. `crates/devflow-core/src/liveness.rs:572–576` already documents this exact problem and isolates affected tests in child processes. The supervisor also explicitly sets close-on-exec in `liveness.rs:275–276`.

   Finding no non-CLOEXEC descriptors cannot reject the hypothesis. Require a child deliberately held between fork and exec. D-07’s permanent-holder timeout control verifies timeout behavior; it does not validate the proposed causal diagnosis. Likewise, isolated nextest repetitions under unrelated CPU load would not recreate sibling threads inheriting descriptors.

4. **Major — D-16’s live “proof” cannot distinguish branch protection from unconditional retention.**

   `.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:98–99`: cleanup keeps the finished run’s unmerged branch; “This exercises D-10 live.”

   Existing cleanup already keeps **every stopped run’s worktree** before checking merge status: `crates/devflow-cli/src/commands.rs:1227–1231`. Finished runs are deliberately recorded as stopped under `.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md:253–258`.

   Therefore observing retention after this run does not establish that cleanup inspected unmerged commits. It can pass while the merge-protection implementation is absent. Add the opposing case: an otherwise equivalent finished, clean, fully merged worktree must be removable under D-09’s new policy. Assert preservation of the unmerged branch’s exact tip, worktree contents, and state—not merely a reassuring message.

## SUSPECTED

5. **Major — Cleanup/discard can race the newly permitted restart.**

   Target: `.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:63–69`, especially “merged, clean, no live run.”

   Current cleanup loads state once, probes liveness, and later removes the worktree without claiming the foreground phase holder: `crates/devflow-cli/src/commands.rs:1199–1218`, `1238–1281`. Start claims that holder at lines 434–447.

   **Unreproduced interleaving:** cleanup observes a finished, clean, merged phase; a concurrent `start` reuses that worktree and reserves or launches a new run; cleanup proceeds using its old observations and removes the worktree. Explicit discard introduces the same gap between listing resources and destroying them.

   The context specifies safety predicates without their synchronization boundary. Require shared exclusion with launch, revalidation while excluded, and a barrier test that starts a launch between cleanup’s observation and removal.

6. **Major — D-13 can fix the manifest while leaving other checkout-dependent checks wrong.**

   `.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:82–86` specifies the pushed SHA and manifest, but its two acceptance scenarios do not distinguish all remaining inputs.

   `scripts/check-workspace-divergence.sh` also reads:

   - The checkout’s exclusion policy at line 30.
   - `HEAD` for comparisons at lines 79 and 87.
   - The checkout’s **index** for executable modes at lines 95–99.

   **Unreproduced partial-fix failure:** switch the manifest and diffs to the pushed commit, but retain `git ls-files -s`. Push a workspace commit containing a declared script change with mode `100644` from a checkout whose index records `100755`. The mode guard examines the wrong object and can accept the broken script.

   Require tests where checkout and pushed commit disagree on executable modes and policy inputs. Also specify deletion handling: workspace-ref deletion supplies an all-zero local SHA, which must not be inspected as a commit. The existing hook already handles that distinction elsewhere at `scripts/hooks/pre-push:85`.

7. **Major — The proof fixture is not pinned, and its default exposes a documented finish blocker.**

   `.planning/phases/52.1-stabilize-for-dogfooding/52.1-CONTEXT.md:92–95` selects `scripts/scratch-dogfood-repo.sh` without selecting its mode or required configuration.

   The default scaffold commits `"workflow": {}` at `scripts/scratch-dogfood-repo.sh:1251–1258`. Startup does not normalize an absent chain flag: `crates/devflow-core/src/gsd_config.rs:144–145` returns without writing when absent already reads as false.

   Prior-phase D-46 explicitly records that GSD Plan adds the absent flag uncommitted, leaving Validate dirty until a commit: `.planning/phases/52-subtract-and-stabilize/52-CONTEXT.md:423–427`. The clean-tree requirement remains enforced in `crates/devflow-cli/src/pipeline_outcomes.rs:1191–1206`.

   **Runtime consequence remains unverified:** if Plan performs that documented write and subsequent stages leave it uncommitted, the default scaffold can start legally with Claude but cannot finish the proposed uninterrupted proof.

   The alternative `--live-payload` includes the committed flag, but also inserts a `blocking-human` checkpoint at `scripts/scratch-dogfood-repo.sh:915`. Pin a fixture mode, configuration, permitted gates, and a dirty-config negative control before calling this an executable exit criterion.
