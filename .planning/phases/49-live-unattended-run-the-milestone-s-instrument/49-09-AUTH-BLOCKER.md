# 49-09 authentication gate — blocked, 2026-09-30

Plan 49-08 is complete. Plan 49-09 has committed launcher code and passing fake tests, but is
not complete and has no SUMMARY: its real authentication gate failed. Do not re-execute the
production tasks or write a completion summary over this gate.

## Verified outcome

- Launcher selftest: 16 passed, 0 failed. Task-2 gate: gate_rc=0; no forbidden gate-response writes.
- Real prepare: exit 0, pinned binary sha matches, mode 0555, payload CHECK passed=20 failed=0,
  current-attempt=1. The real gate's temporary home was removed by its EXIT trap.
- Real auth probe: probe_exit=1, is_error=true, reply_ok=no, auth_source=none; overall Task-1 gate_rc=1.
- Environment names: 107 lines and independently 107 unique names; no NAME=value lines; PATH,
  HOME and DEVFLOW_GATE_NOTIFY_CMD all present. Session names=0; caller_claude_names=1.
  CLAUDECODE=P49_SESSION_NEGATIVE_CONTROL was deliberately planted only in the caller's
  process environment to make the session-isolation check non-vacuous.
- No authprobe scratch directories remained. The transient p49-a1-authprobe unit was independently
  absent from systemctl list-units after collection. No positive start/watcher unit was launched.

These checks prove one successful preparation and one failed authentication probe. They do not
establish the cause of the auth failure, usable credentials, account/billing ownership, positive
live-run readiness or reliability. Presence of credential inputs is not evidence that they work.

## Name-only credential inputs

The unit recorded ANTHROPIC_API_KEY and ANTHROPIC_TOKEN. It did not record ANTHROPIC_AUTH_TOKEN,
ANTHROPIC_BASE_URL, CLAUDE_CODE_OAUTH_TOKEN, CLAUDE_CODE_USE_BEDROCK, CLAUDE_CODE_USE_VERTEX,
CLAUDE_CODE_USE_FOUNDRY, CLAUDE_CONFIG_DIR or ANTHROPIC_PROFILE.

User settings ~/.claude/settings.json exist: env key names ANTHROPIC_API_KEY and CLAUDE_CODE_SHELL;
apiKeyHelper key absent. /etc/claude-code/managed-settings.json is absent. ~/.claude/.credentials.json
exists; ~/.config/anthropic does not. The scaffold creates no project .claude settings; its temporary
project was deleted. Only existence and key names were inspected, never credential values.

## Durable and temporary evidence

Production commits: 91fc3b9 (RED three cases), cba140c (implementation), c568940 (13 additional controls).
Raw agent stdout/stderr were discarded by the tool as required. Safe derived fields and environment
names were retained in /tmp/49-09-diag.Jx0knZ; the console gate report is /tmp/49-09-real.log and the
Task-2 report /tmp/49-09-gate2.log. Temporary paths may disappear after reboot; this document preserves
all decisive facts. No credential values or raw agent captures enter this repository.

## Pending operator decision

Which credential route should the nested agent use: the existing Claude login or an API/provider
configuration? Do not select one, edit settings or import/clear manager variables until answered.
Plan 49-09 expressly requires stopping on this real-probe failure; preserve that checkpoint.

## Resume sequence

1. Obtain the credential-route decision and diagnose the chosen route without displaying secrets.
2. Apply only the authorized auth configuration changes, then rerun the real prepare/auth gate.
3. Before a positive start, correct a pending inspection finding: start currently caches base/main
   SHAs for push validation before the auth probe and then writes those cached values afterwards.
   The plan requires re-reading the SHAs after the probe, immediately before recording the fork.
   This has not been exercised as a failure; fix and add a mutation control before live launch.
4. Rerun affected gates; only then write 49-09-SUMMARY and advance. Plans 49-10..49-12 remain unstarted.

The gate command was extended only to preserve the two safe diagnostic files before deleting its
temporary home; production behavior and the plan artifact were unchanged. It used process-local
session-bus variables to reach the existing UID-1000 manager. No credentials, settings or manager
configuration were changed.
