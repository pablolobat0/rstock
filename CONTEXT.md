# rstock

rstock tracks a portfolio in EUR using transaction history, daily market data, and NAV unitization.

## Language

**NAV**:
The time-weighted value per portfolio share used to measure portfolio performance independent of cash-flow timing.
_Avoid_: Portfolio value, return

**Effective valuation date**:
The latest date for which NAV can be calculated using available prices and FX rates for every holding required by the portfolio.
_Avoid_: Latest price date, today

**Individual price**:
The latest available quoted price for one asset, independent of whether every other holding has data for the same date.
_Avoid_: NAV price

**Stale market data**:
Previously cached market data that is older than the requested valuation date but still usable to calculate NAV at an earlier effective valuation date.
_Avoid_: Missing market data

**Historical market data**:
Persisted market data for completed dates, used to build reproducible NAV history.
_Avoid_: Live quote

**Live quote**:
Non-persisted same-day stock price or FX rate used for current display before the market day is complete.
_Avoid_: Historical market data

**Market data limitation**:
A reason market data could not support the requested valuation date, such as stale cached data or missing required data.
_Avoid_: Log message

**Base currency**:
The currency in which portfolio values, NAV, and aggregate returns are expressed.
_Avoid_: Local currency, reporting currency

**Acceptable Morningstar lag**:
A fund or ETF price delay of seven days or less on the Morningstar price path that can limit NAV without requiring a user-facing warning.
_Avoid_: Fresh fund price, acceptable stock lag

**Completed weekday**:
A date before today that falls on Monday through Friday, used as the simple expected cadence for stock and FX historical market data.
_Avoid_: Trading day, market calendar day

**Forward-filled market data**:
Persisted market data for a completed non-trading date copied from the most recent earlier source value.
_Avoid_: Live quote, extrapolated price

## Relationships

- **NAV** is calculated at one **Effective valuation date**.
- An **Effective valuation date** is constrained by the oldest latest-available market data needed across all holdings and FX rates.
- An **Individual price** may be newer than the **Effective valuation date**.
- **NAV** cannot be calculated when any held asset or required FX rate has no market data for the valuation period.
- **Stale market data** may move the **Effective valuation date** earlier.
- **NAV** uses **Historical market data**, not **Live quote** values.
- An **Individual price** for a stock may use a **Live quote** for display.
- Mutual funds use a single closing price and do not use **Live quote** values for display.
- An **Individual price** display value may combine a stock **Live quote** with stale cached FX when live FX is unavailable, but that creates a **Market data limitation**.
- A completed date is any date before today; same-day market-close calendars are intentionally not used.
- A **Market data limitation** is part of the result of preparing market data, not only diagnostic logging.
- Price lookup identity is part of market data: stocks use ticker; funds and ETFs use Morningstar code.
- A held fund or ETF without a Morningstar code cannot provide required **Historical market data** for **NAV**.
- Benchmark market data follows the same historical availability rules as holdings, but a benchmark is not a holding.
- Market data can be represented as a native asset price, an FX rate, and a EUR valuation price for valuation and audit.
- **Forward-filled market data** is allowed only between source observations and never beyond the last date returned by the source.
- The **Base currency** has an implicit FX rate of 1.0.
- **Acceptable Morningstar lag** affects warning visibility, not **NAV** calculation.
- Stock and FX stale-data warnings are based on **Completed weekday** cadence, not exchange-specific holiday calendars.

## Example Dialogue

> **Dev:** "MSFT has a price for today, but the ETF only has data through yesterday. Should NAV use today's MSFT price?"
> **Domain expert:** "No. NAV uses the effective valuation date where all required prices and FX rates are available; individual prices may still show their own latest quote."

## Flagged Ambiguities

- "latest price" can mean either **Individual price** or the market data limiting the **Effective valuation date**; use the precise term.
- Missing market data for a held asset or required FX rate is not the same as stale market data; missing data stops **NAV** calculation, while stale data may move the **Effective valuation date** earlier.
