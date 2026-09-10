# 01 — Establish the prepared NAV rebuild plan

## Parent

#62

## What to build

Make public NAV readiness prepare one immutable, NAV-owned rebuild plan before calculation. The plan must own the NAV rebuild checkpoint state, ordered Transaction ledger entries, required Tracked assets, and range-loaded valuation data needed by the existing rebuild. Execute the complete rebuild range from that prepared state without database or MarketData reads, while preserving all existing calculable NAV values, calendar-date coverage, limitations, and unitization behavior.

## Acceptance criteria

- [ ] Public NAV readiness remains the only caller-facing and behavioral test seam; no planner or executor interface is made public solely for tests.
- [ ] One immutable NAV-owned plan contains the checkpoint state, transactions in date and ascending-ID order, required assets, and prepared price and FX series for the complete rebuild range.
- [ ] After preparation returns the plan, calculation performs no repository, database-read, or MarketData calls.
- [ ] Captured database work at the public readiness seam proves execution has zero reads rather than relying only on private-helper tests.
- [ ] Missing in-memory valuation data during execution is an invariant failure and never triggers a fallback lookup.
- [ ] Existing initialization, buy, sell, fee, dividend, split, liquidation, currency-conversion, weekend, and per-asset snapshot scenarios retain equivalent persisted results.
- [ ] Complete NAV snapshot date coverage and existing NAV-scoped Market data limitations remain unchanged for currently calculable scenarios.
- [ ] The implementation prepares one complete in-memory range and does not introduce windowed or streaming plans.
- [ ] Tests use fixed clocks, dummy Tracked asset identities, in-memory SQLite, and fake Market data sources without network access.
- [ ] Formatting, linting with warnings denied, and the complete test suite pass.

## Blocked by

None — can start immediately.
