# Fixture: unterminated block

An opening tag with no closing tag anywhere after it. Left unhandled this is a bypass
by construction: a parser that only matches complete blocks sees nothing to check.

<verify>
  <automated>
echo "${PIPESTATUS[0]}"
</verify>
