# Phase 53.2 deferred items

## From 53.2-13

- Operator docs still name the removed supervisor as a live component: OPERATIONS.md (16 lines, e.g. :509 "the waiting supervisor relaunches the stage"), ARCHITECTURE.md (3), docs/guides/unattended-mode.md (5), docs/guides/configuration.md (1), counted with `rg -c -i supervisor` on 2026-11-23. Some may be history; each needs reading. Plan 13's scope (#324) was code texts only: stop reasons, event sources, gate texts and clap help. Candidate owner: a #324 follow-up docs pass.
