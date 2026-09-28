## CONFIRMED

Paths below are relative to `snapshot/.planning/phases/49-live-unattended-run-the-milestone-s-instrument/`.

1. **Critical — the plans require their own SUMMARY before GSD creates it.** `49-10-PLAN.md:268`, `49-11-PLAN.md:136`, and `49-12-PLAN.md:163` put “this plan’s SUMMARY” in a later task’s `<read_first>`. GSD requires those reads before the task (`execute-plan.md:225`) but creates the SUMMARY only after **all** tasks (`execute-plan.md:406-412`). At closeout, `49-18-PLAN.md:193` makes its Task 2 gate require a triage heading in `49-18-SUMMARY.md` immediately after merging the PR—again before GSD creates that file. The legitimate sequence therefore blocks. Writing a draft SUMMARY to get past it is unsafe: GSD counts a matching SUMMARY file as completion (`phase.cjs:800-821`) and can skip the plan after an interruption.

2. **High — a Code-stop retry approves Define before checking the new attempt’s Define evidence.** `49-10-PLAN.md:142-147` tells the operator to launch N+1 and approve Define outside GSD, then resume only when Code opens. The binary, arm-pair, and Define-context checks are deferred to `49-11-PLAN.md:100-111`. On attempt 1, those checks precede the approval (`49-10-PLAN.md:244-252,270-279`). Input: attempt N+1 has a bad positive arm or wrong Define context. Outcome: it crosses Define anyway; 49-11 discovers the failure only at Code. This is a control-flow finding from the instructions, not a live-run observation.

3. **High — snapshot gates can judge an old request instead of the request recorded by the watcher.** The Define gate selects the lexicographically first `define-*` file (`49-10-PLAN.md:255`); the Code gate does the same for `code-*` (`49-11-PLAN.md:123`). Neither binds that file to the ledger entry’s `snapshot` path or `sha256`, although the watcher records both (`49-03-PLAN.md:125-132`) and explicitly supports changed content under the same gate name (`49-03-PLAN.md:186-189`). In a temporary fixture, the ledger pointed to `define-200`, whose context contained the forbidden config condition. The plan’s selection fragment chose `define-100` and printed `pass=yes`. Changing only `define-100` to the bad context printed `pass=no`. **Limit:** this exercised the snapshot-selection predicates, not the full gate, whose live system calls were prohibited.

4. **Medium — a fabricated stop disposition passes the final attempt gate.** `49-10-PLAN.md:120-131` requires a seven-field failed-attempt record. `49-15-PLAN.md:190` checks its attempt number and stage, but not `plan`, `cause`, `counts_as_attempt`, `disposition`, or the operator reply. I ran that full second automated gate in a temporary Git fixture. An earlier outcome containing `plan: invented-plan`, `cause: banana`, and `disposition: success` passed with `gate_rc=0` when its `stage: define` matched the ledger. Changing the stage to `code` produced `gate_rc=1`. The stage control works; the rest of the stated record contract does not.

5. **Medium — the verdict gate can mark passing C1/C2 evidence `NOT_MET`.** `49-15-PLAN.md:190` requires a passing check file when C1 or C2 says `MET`, but imposes no reverse requirement when either says `NOT_MET`. Task 2 then derives VERIFY-01’s status from those tokens (`49-15-PLAN.md:210-213,225`). In the same full-gate fixture, C1 and C2 were `NOT_MET`, both assigned check files ended `CHECK passed=2 failed=0`, and the gate returned `gate_rc=0`. As a negative control, an invalid C1 check paired with `C1: MET` returned `gate_rc=1`. Thus the gate detects unsupported success but permits a false failure to drive traceability.

## SUSPECTED

None retained as a finding.

**What I did not check:** I did not run DevFlow, GSD execution, systemd, `gh`, containers, network calls, or a full cargo build. The tool scripts planned for earlier waves are absent from this snapshot, so the snapshot finding does not establish how their eventual implementation behaves in a live attempt.


