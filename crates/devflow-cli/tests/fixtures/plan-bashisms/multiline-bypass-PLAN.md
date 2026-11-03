# Fixture: multi-line bypass (codex's first proven bypass)

Committed successfully through the live pre-commit hook before 46-05. The old scanner
required the bashism on the same line as the opening tag, so a block spanning lines
escaped it entirely.

<verify>
  <automated>
echo "${PIPESTATUS[0]}"
  </automated>
</verify>
