# 01 — Establish a deterministic current-date seam

**What to build:** Make the meaning of "today" consistent and injectable across Portfolio view assembly, NAV readiness, and Individual price behavior. Production continues to use system time, while tests can fix time without changing every caller or relying on the machine calendar.

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

- [ ] Production portfolio, NAV, and Individual price behavior obtains the current date through one clock seam rather than constructing system time independently.
- [ ] Production wiring uses a system-time adapter without requiring users or command callers to pass a date.
- [ ] Tests can inject a fixed-time adapter and deterministically control current inventory cutoffs, completed dates, and price-date behavior.
- [ ] The system-time and fixed-time adapters are the only adapters introduced for this seam.
- [ ] Existing user-visible behavior remains unchanged when production uses the system-time adapter.
- [ ] Existing no-network market data test support remains usable with the fixed clock.
- [ ] Focused tests demonstrate that future-dated Transaction ledger entries and latest completed dates are evaluated relative to fixed time.
- [ ] Formatting, strict linting, and the complete test suite pass.
