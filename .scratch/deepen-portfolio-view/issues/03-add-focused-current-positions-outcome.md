# 03 — Add a focused current-positions outcome

**What to build:** Add a focused portfolio outcome for current Transaction ledger inventory, separate from NAV, returns, and risk facts. Use one ledger projection for performance positions and Monetary holdings while keeping existing Portfolio view callers operational during the expansion.

**Blocked by:** 01 — Establish a deterministic current-date seam

**Status:** ready-for-agent

- [ ] The portfolio module interface exposes focused current positions without unrelated NAV, return, risk, or chart facts.
- [ ] The focused outcome derives open holdings from ordered Transaction ledger entries through the fixed current date.
- [ ] One internal projection derives quantity, remaining cost, dividends, and split/sell effects for both performance positions and Monetary holdings.
- [ ] Performance positions and Monetary holdings use one position fact representation while remaining in separate collections.
- [ ] Monetary holdings remain excluded from performance-position aggregates.
- [ ] Requesting focused current positions does not create or rebuild NAV history.
- [ ] Existing full Portfolio view behavior remains available while callers migrate.
- [ ] Tests exercise the focused outcome through the portfolio module interface rather than exposing a public pure ledger helper.
- [ ] Empty inventory, future transactions, post-Effective-valuation-date buys, and split-adjusted quantities are covered with fixed dates.
- [ ] Formatting, strict linting, and the complete test suite pass.
