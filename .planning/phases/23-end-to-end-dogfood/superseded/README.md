# Superseded plans — phase 23

These ten plans (23-03 … 23-12) were written before plan 23-02's probe ran.
They were built on the premise that the `sh -c` monitor's death is what blocks
an unattended end-to-end DevFlow run.

The probe **invalidated that premise** at its blocking scope gate on 2026-07-25
(operator signal: `invalidated: replan 23`). Evidence:

- `../23-PROBE-FINDINGS.md` — the monitor survived the whole run; Ship failed on
  a content review, not a process failure.
- `../23-ORPHAN-FORENSICS.md` — the real defect is monitor *immortality*
  (`devflow advance` blocks forever on a pending gate), not monitor death.

They are retained unmodified for provenance. Plans 23-01 and 23-02 were executed
and merged and remain in the phase directory. Do not execute anything in here.
