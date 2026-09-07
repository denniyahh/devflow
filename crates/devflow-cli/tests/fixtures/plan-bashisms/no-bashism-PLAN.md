# Fixture: ordinary block, no bash-only syntax

Second negative control: proves the scanner is not simply reacting to the presence of
a verify block.

<verify>
  <automated>cargo fmt --check</automated>
</verify>
