## Deferred Items

- `reset_time::parse_reset_time` accepts a day past the month's end (found in 52-01)
  status: open
  **What:** the RFC 3339 path checks `day <= 31` only, so `2026-02-31T00:00:00Z` reads as 2026-03-03 (probed, equal epoch values). The check came over unchanged from `ship.rs::parse_rfc3339ish`.
  **Impact:** the result is a later instant, never an earlier one, so a relaunch cannot fire before the named time. It is still a date the agent never meant.
  **Fix shape:** refuse a day past the month's length (leap-aware) and add a test for Feb 29 on leap and non-leap years. Fits plan 52-31 or 52-32, where the rate-limit gate consumes the parser.
