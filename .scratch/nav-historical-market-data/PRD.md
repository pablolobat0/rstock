# PRD: Market Data Over Calendar Dates

Status: done

## Problem Statement

NAV, individual price display, benchmark analysis, and EUR valuation all depend on daily asset prices and FX rates, but the rules for preparing and interpreting that market data are currently spread across several shallow Modules. Asset price filling, FX filling, fallback to cached data, live quote fetching, forward-fill behaviour, effective valuation date calculation, and missing-data handling live in different places. This makes market data hard to reason about and allows a dangerous case: a held asset or required FX rate with no market data can be silently omitted from NAV, producing an understated NAV instead of failing clearly.

The user needs NAV to remain conservative and auditable: it should be calculated only at a single effective valuation date where every required holding and required FX rate has historical market data. Individual prices may be newer than NAV and may use live quotes for display, but NAV must not mix dates or omit holdings. Benchmark market data should follow the same historical availability rules as holdings without pretending the benchmark is a holding.

## Solution

Create a deeper market-data Module over calendar dates. The Module will own historical market-data preparation, individual price display data, benchmark market-data availability, base-currency conversion, forward-filled market data, effective valuation date calculation, and structured market data limitations.

NAV will continue to decide which holdings and FX pairs are required for the rebuild, but it will delegate market-data availability, fallback, and effective valuation date calculation to the market-data Module. Missing required market data will become a hard stop for NAV. Stale cached market data may still be used, but it will move the effective valuation date earlier and be returned as a market data limitation.

Individual price display will also use the market-data Module, but with different rules: stocks and FX may use non-persisted live quotes for current display, while funds and ETFs on the Morningstar price path use their single closing price and do not use live quotes. Benchmark market data will use the historical availability workflow and remain distinct from holdings.

## User Stories

1. As a portfolio owner, I want NAV to use one effective valuation date, so that portfolio performance is calculated from a coherent set of prices and FX rates.
2. As a portfolio owner, I want NAV to stop when a held asset has no market data, so that my NAV is never understated by an omitted holding.
3. As a portfolio owner, I want NAV to stop when a required FX rate has no market data, so that non-EUR holdings are not valued with an implicit or incorrect rate.
4. As a portfolio owner, I want stale cached market data to remain usable, so that temporary fetch failures do not prevent NAV from being calculated for the latest fully supported date.
5. As a portfolio owner, I want stale cached market data to move the effective valuation date earlier, so that NAV does not pretend to be current when one required data source lagged.
6. As a portfolio owner, I want missing market data and stale market data to be treated differently, so that I know whether the system cannot calculate NAV or can calculate NAV at an earlier date.
7. As a portfolio owner, I want funds and ETFs without Morningstar codes to fail NAV preparation clearly, so that I know asset metadata must be fixed.
8. As a portfolio owner, I want stocks to use ticker-based price lookup, so that stock prices continue to come from the existing stock price path.
9. As a portfolio owner, I want funds and ETFs to use Morningstar-code price lookup, so that Morningstar-backed historical prices continue to work.
10. As a portfolio owner, I want completed historical dates to be persisted, so that NAV history remains reproducible.
11. As a portfolio owner, I want same-day live quotes excluded from NAV, so that NAV history does not depend on partial intraday data.
12. As a portfolio owner, I want same-day market-close calendars kept out of market-data cacheability rules, so that the behaviour stays simple and predictable.
13. As a portfolio owner, I want forward-filled market data for weekends and holidays between source observations, so that NAV can be calculated on calendar days without inventing prices beyond source data.
14. As a portfolio owner, I want forward-filled market data never to extend beyond the last source observation, so that NAV does not extrapolate unavailable market data.
15. As a portfolio owner, I want EUR assets to use an implicit FX rate of 1.0, so that base-currency holdings are valued consistently.
16. As a portfolio owner, I want non-EUR assets to require FX market data, so that all aggregate values remain in the base currency.
17. As a portfolio owner, I want the base currency to remain EUR, so that the implementation matches the project’s current domain rule.
18. As a portfolio owner, I want market-data preparation to return structured market data limitations, so that callers can decide how to present stale-data conditions.
19. As a portfolio owner, I want normal Morningstar lag of seven days or less not to create noisy warnings, so that expected fund and ETF reporting delays do not distract me.
20. As a portfolio owner, I want Morningstar lag greater than seven days to be visible as a limitation, so that unusually delayed fund or ETF data is actionable.
21. As a portfolio owner, I want stock and FX stale-data warnings to ignore weekends, so that non-trading weekends do not produce unnecessary warnings.
22. As a portfolio owner, I accept that exchange holidays may still produce occasional stale-data warnings, so that market-data handling does not need a market-calendar Module yet.
23. As a maintainer, I want market-data availability rules in one Module, so that bugs in NAV data preparation have strong locality.
24. As a maintainer, I want NAV holdings selection to stay in NAV, so that portfolio state rules do not leak into market-data preparation.
25. As a maintainer, I want market-data preparation to keep fetch, fill, persist, and outcome derivation behind one Interface, so that callers do not have to know the required ordering.
26. As a maintainer, I want asset prices and FX rates to share the same availability workflow, so that duplicated cache and fallback logic is reduced.
27. As a maintainer, I want asset prices and FX rates to remain distinct internal data types, so that their different storage shape and identity rules stay explicit.
28. As a maintainer, I want market-data preparation to return native price, FX rate, and EUR valuation price where relevant, so that callers can audit valuation without reimplementing conversion policy.
29. As a maintainer, I want portfolio asset history reuse to remain outside market-data preparation, so that snapshot persistence behaviour does not contaminate the market-data seam.
30. As a maintainer, I want same-range fetching preserved unless a later issue explicitly optimizes per-subject ranges, so that the deeper Module is not forced to expose unnecessary caller complexity.
31. As a maintainer, I want benchmark market data to use the same historical availability workflow as holdings, so that analytics does not duplicate price and FX preparation rules.
32. As a maintainer, I want benchmark market data to remain distinct from holdings, so that analysis does not contaminate portfolio state.
33. As a maintainer, I want individual price display to use the same market-data Module, so that live quote, stale FX, and latest price rules are local.
34. As a maintainer, I want individual display values to carry market data limitations, so that display can show useful values without hiding stale FX or source lag.
35. As a maintainer, I want tests to exercise the market-data Module through its public Interface, so that the test surface matches the Module seam.
36. As a maintainer, I want missing-data tests to fail before NAV writes partial snapshots, so that dangerous valuation errors are caught directly.
37. As a maintainer, I want stale-cache tests to prove the effective valuation date moves earlier, so that offline resilience remains intentional.
38. As a maintainer, I want benchmark tests to prove benchmark prices and benchmark FX use historical availability rules, so that benchmark analysis does not remain a separate market-data path.
39. As a maintainer, I want live display tests to prove live stock and FX quotes are not persisted as historical market data, so that same-day display does not corrupt reproducible NAV history.

## Implementation Decisions

- Build or modify a deep market-data Module that owns market data over calendar dates for NAV, individual price display, and benchmark analysis.
- The market-data Module should expose separate workflows behind the same Module for NAV historical preparation, individual display data, and benchmark historical data.
- The market-data Module Interface should prepare required historical market data for a valuation request and return an outcome, not expose separate public fetch and persist operations for callers to compose.
- NAV remains responsible for selecting required assets and required FX pairs from previous holdings and transactions in the rebuild range.
- The market-data Module owns effective valuation date calculation from latest available required asset and FX data.
- The market-data Module owns the distinction between missing market data and stale market data.
- Missing required market data is a hard stop for NAV.
- Fetch failure with existing cached data is stale but usable; it may move the effective valuation date earlier.
- Historical market data is persisted only for completed dates, meaning dates before today.
- Same-day live quotes are not used for NAV and are not persisted as historical market data.
- Forward-filled market data remains persisted for completed dates between source observations.
- Forward-filled market data must never extend beyond the last date returned by the source.
- Asset price lookup identity is owned by market data: stocks use ticker, funds and ETFs use Morningstar code.
- A held fund or ETF without a Morningstar code is a hard market-data preparation error for NAV.
- Asset prices and FX rates should share one availability workflow but remain separate internal concepts.
- The base currency remains EUR and has an implicit FX rate of 1.0.
- Non-EUR assets require explicit FX market data for NAV valuation.
- Market-data preparation should return structured market data limitations rather than relying only on logging.
- User-facing warning policy should distinguish normal Morningstar lag from actionable stale data.
- Acceptable Morningstar lag is seven days or less and affects warning visibility only, not NAV calculation.
- Stock and FX stale-data warnings use completed weekday cadence, not exchange-specific holiday calendars.
- No explicit market-calendar Module will be introduced.
- No schema change is planned.
- Existing repository Modules remain data access Modules; business rules move to the market-data Module rather than into repositories.
- Existing fetch adapters remain thin and continue to satisfy the current price-fetching seam.
- Portfolio asset history reuse remains outside the market-data Module.
- Benchmark market data follows the same historical availability rules as holdings, but a benchmark is not a holding.
- Individual price display may use live quotes for stocks and FX without persisting them as historical market data.
- Individual price display may combine a stock live quote with stale cached FX when live FX is unavailable, but that produces a market data limitation.
- Funds and ETFs on the Morningstar price path do not use live quotes for display.

## Testing Decisions

- Good tests should exercise externally visible behaviour through the market-data preparation Interface and NAV rebuild behaviour, not private helper details.
- Test the market-data Module for effective valuation date calculation across multiple assets and FX rates.
- Test the market-data Module for missing required asset market data causing a hard failure.
- Test the market-data Module for missing required FX market data causing a hard failure.
- Test the market-data Module for fetch failure with existing cached data producing stale but usable market data.
- Test the market-data Module for fetch failure with no cached data producing a hard failure for required data.
- Test the market-data Module for missing Morningstar code on a held fund or ETF producing a clear hard failure.
- Test the market-data Module for EUR assets using implicit FX rate 1.0.
- Test the market-data Module for non-EUR assets requiring FX data.
- Test forward-filled market data persistence between source observations.
- Test that forward-filled market data is not created beyond the last source observation.
- Test that the effective valuation date is the minimum latest available date across all required assets and required FX rates.
- Test that stale market data returns structured market data limitations.
- Test warning classification for Acceptable Morningstar lag versus Morningstar lag greater than seven days.
- Test warning classification for stock and FX stale data using completed weekday cadence.
- Test NAV rebuild does not write partial snapshots when required market data is missing.
- Test benchmark market data preparation uses historical availability rules and required benchmark FX.
- Test benchmark market data remains distinct from holdings.
- Test individual display data can use non-persisted live stock and FX quotes.
- Test individual display data does not use live quotes for funds and ETFs on the Morningstar price path.
- Test individual display data can return a live stock price with stale cached FX plus a market data limitation.
- Use in-memory SQLite test setup and dummy tickers.
- Reuse the existing mock price fetcher approach.
- Prior art exists in daily price tests for cached price lookup and forward-fill behaviour.
- Prior art exists in NAV tests for rebuild behaviour, weekend forward-fill, and multi-asset valuation.
- Prior art exists in portfolio summary tests for current position and NAV result behaviour.

## Out of Scope

- Correlation, risk metric, and broader performance-series refactoring are out of scope except where they consume benchmark market data prepared by the market-data Module.
- Per-subject fetch ranges are out of scope.
- Configurable base currency is out of scope; EUR remains the base currency.
- Explicit exchange holiday calendars are out of scope.
- Market-close-aware same-day historical caching is out of scope.
- Portfolio asset history reuse changes are out of scope.
- Database schema changes are out of scope.

## Further Notes

- This PRD comes from the architecture deepening candidate “Market Data Over Calendar Dates.”
- The primary architectural goal is to replace several shallow Modules and duplicated rules with a deeper market-data Module whose Interface is the test surface.
- The highest-risk current behaviour is silent omission of holdings or FX data during NAV calculation; implementation issues should prioritize making that impossible.
- The accepted trade-off for stale-data warnings is weekend-aware but not holiday-aware. This is documented in domain language and does not require an ADR because it is easy to reverse later.
- The current domain glossary is recorded in `CONTEXT.md`; implementation should continue using that vocabulary.
