# Fixture: generic array index

zsh indexes arrays from 1, so `${myarr[0]}` silently yields the wrong element rather
than erroring. Same false-green class as PIPESTATUS.

<verify>
  <automated>myarr=(a b c); echo "first=${myarr[0]}"</automated>
</verify>
