# Phase 51: Run Notes (stages run outside DevFlow)

Per the dogfooding rule in CLAUDE.md, every stage or step run outside `devflow start` is recorded here with its reason.

## Exception 1: first review fix round (2026-11-08)

- **What ran by hand:** `/gsd-code-review 51 --fix`, then `--fix --all` (operator: "run the fix with --all"), in an
  agent session. Fixer commits `9886a95`..`c6e29e0`, report `42a7269`, disposition `09eb6e2`.
- **Why outside DevFlow:** DevFlow 3.0.0's Validate reject loops back to Code but cannot pass the operator's note to the
  agent (999.189), so a reject cannot ask for "fix the review findings".
- **Operator approval:** yes, by the requests quoted above.

## Exception 2: second fix round, after the external review (2026-11-08)

- **Trigger:** the Validate gate fired "Validation passed — approve to ship?" while `51-VERIFICATION.md` reported
  `gaps_found` (999.193), the GSD re-review had 6 open findings, and `51-EXTERNAL-CODE-REVIEW.md` confirmed 4 major
  findings. Operator chose "fix round, then reject the gate again".
- **What ran by hand:**
  - `/gsd-code-review 51 --fix --all` (GSD route: empty scope → dispatch-fix): commits `5ced4d8`..`e0683ce`, report
    `e594ff8`, disposition `0f06cdf` (0 of 6 open; IN-02 closed as a record against 999.187, no code change).
  - A fix agent for the external findings: `73303a9` (failover test inside a phase worktree, X-03), `ee7451a` (X-05),
    `e4bf0c5` (X-06), `b94d356` (gate list context, X-08), `be243d3` (SUPV-01 evidence citations re-derived, X-04).
  - X-01 (denial followed by a torn line): the phase decisions did not settle which rule wins; operator chose option B
    (the torn line wins everywhere, the stage ends at a gate): `17a47cf`. Same round: `91e1b76` makes the worktree
    handoff test assert DevFlow's `.planning/config.json` change is only the auto-chain flag (operator raised it; the session made the change).
  - Not fixed here, filed to backlog: X-02 → 999.194 (Phase 52, operator), X-07 → 999.195, X-09 → 999.196, X-10 → 999.197, X-11 → 999.198,
    X-12 → 999.199, suspected items → 999.200-999.202, 999.204, checker gap → 999.203.
- **Why outside DevFlow:** same as exception 1 (999.189), and a reject would dispatch a full execute rather than gaps
  only (999.192).
- **Next step inside DevFlow:** reject the Validate gate (note without "abort") so Validate re-runs on the fixed code.

## Exception 3: third fix round, at the Code failure gate (2026-11-08)

- **Trigger:** after the second reject, DevFlow re-ran Code as a full execute (999.192). All 13 plans were complete, so
  only GSD's verify tail ran: the re-review (`4a23acf`, 5 findings) and the verifier (`7095b38`, `gaps_found`: OPERATIONS.md
  over-promised the torn-line fix). Code failed and fired a failure gate (`18e6265d44ff61fa`). No DevFlow route plans gap
  closure (999.205), so every gate answer re-runs the same verify tail.
- **Operator chose option A:** document the killed-mid-line residual (WR-02) instead of fixing it in code, fix the four
  small findings, and move the real fix to Phase 52 (999.207). The gate-reason truncation found here went to Phase 52
  (999.206, operator).
- **What ran by hand:** a fix agent: `ff649ff` (WR-02 residual documented), `cce00cc` (WR-01), `d149da8` (IN-01),
  `fc92c6b` (IN-02), `744739a` (IN-03); fix report `e564570`; GSD disposition step `6443b82`.
- **Next step inside DevFlow:** answer the Code failure gate without "abort"; Code re-runs its verify tail.

