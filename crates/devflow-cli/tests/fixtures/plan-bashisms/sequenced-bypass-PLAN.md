# Fixture: sequenced bypass (defeats an ordering rule)

A real `bash -c` precedes the bashism, but its single-quoted region has already closed
by the time the bashism runs — in the OUTER shell. A rule of the form "bash -c precedes
the bashism" accepts this; containment rejects it.

<verify>
  <automated>bash -c ':'; echo "${PIPESTATUS[0]}"</automated>
</verify>
