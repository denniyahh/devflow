# Fixture: BASH_REMATCH

zsh populates `$match`, not `$BASH_REMATCH`, so this expands silently to the empty
string under the executor's shell.

<verify>
  <automated>[[ ab =~ (a)b ]]; echo "cap=${BASH_REMATCH[1]}"</automated>
</verify>
