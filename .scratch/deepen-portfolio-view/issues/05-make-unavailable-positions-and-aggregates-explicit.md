# 05 — Make unavailable positions and aggregates explicit

**What to build:** Keep every current holding visible when market data is incomplete, preserve independently known facts, and make all dependent position facts and aggregates explicitly unavailable instead of omitting holdings, substituting current FX, or returning partial totals.

**Blocked by:** 04 — Correct current-holding financial facts

**Status:** ready-for-agent

- [ ] A currently held performance asset with no Individual price remains visible with Transaction ledger quantity and available cost facts.
- [ ] Performance positions and Monetary holdings expose the same availability semantics for price, price date, current value, remaining cost, dividends, Open-position gain/loss, and percentage facts.
- [ ] Unavailable scalar facts render clearly in human output and serialize as `null` in JSON rather than zero.
- [ ] Historical Base currency cost and dividends use the latest FX rate on or before each transaction date.
- [ ] Missing historical FX never falls back to current or later FX.
- [ ] Missing historical FX makes only affected cost, dividend, and dependent gain/loss facts unavailable; independently available quantity and current value remain visible.
- [ ] Every performance, Monetary, and combined aggregate is complete across its scope or unavailable.
- [ ] Known per-position and independent subtotal facts remain visible when another aggregate is unavailable.
- [ ] Total value is unavailable when either performance-position current value or Monetary holding value is unavailable.
- [ ] The informational Total value may combine different Individual price dates and retains each position's price date without being presented as NAV.
- [ ] NAV/history, current performance-position, and Monetary holding Market data limitation values are returned in separate scopes.
- [ ] Existing omission coverage is rewritten to assert visibility and unavailable facts for an unpriced holding.
- [ ] Interface, human-output, and structural JSON tests cover missing price, missing historical FX, mixed price dates, complete aggregates, and unavailable aggregates.
- [ ] Formatting, strict linting, and the complete test suite pass.
