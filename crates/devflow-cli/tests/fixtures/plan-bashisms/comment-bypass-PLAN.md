# Fixture: trailing-comment bypass (codex's second proven bypass)

Committed successfully through the live pre-commit hook before 46-05. The old scanner
dropped any line *containing* the substring `bash -c`, including in a trailing comment.

<verify>
  <automated>echo "${PIPESTATUS[0]}" # bash -c</automated>
</verify>
