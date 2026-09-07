# Fixture: the legitimate form (load-bearing negative control)

The bashism lies inside the single-quoted region that `bash -c '` opened, so it really
does run under bash. A scanner that rejected this would make plan authoring impossible
and would satisfy every must-fail case here.

<verify>
  <automated>bash -c 'cargo test 2>&1 | tail -3; echo "x=${PIPESTATUS[0]}"'</automated>
</verify>
