# Fixture: double-quoted bypass (defeats an ordering rule)

Double quotes open no single-quoted region, so the OUTER shell expands the bashism
before bash is ever started. Ordering accepts this; containment rejects it.

<verify>
  <automated>bash -c "echo ${PIPESTATUS[0]}"</automated>
</verify>
