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
A user-actionable reason market data could not support the requested valuation date.
_Avoid_: Log message

**Market data source**:
An external origin of market data used by rstock, such as Yahoo Finance or Morningstar.
_Avoid_: Provider, API

**Base currency**:
The currency in which portfolio values, NAV, and aggregate returns are expressed.
_Avoid_: Local currency, reporting currency

**Acceptable Morningstar lag**:
A fund or ETF price delay of seven days or less on the Morningstar price path that can limit NAV without creating a Market data limitation.
_Avoid_: Fresh fund price, acceptable stock lag

**Completed weekday**:
A date before today that falls on Monday through Friday, used as the simple expected cadence for stock and FX historical market data.
_Avoid_: Trading day, market calendar day

**Forward-filled market data**:
Persisted market data for a completed non-trading date copied from the most recent earlier source value.
_Avoid_: Live quote, extrapolated price

**Portfolio-relevant analysis**:
Analysis that explains existing portfolio holdings, portfolio performance, or candidate holdings being tracked for possible portfolio action.
_Avoid_: General market research, standalone stock screener

**Fund candidate**:
A fund or ETF being analyzed for possible inclusion in the portfolio, even before it exists as a portfolio asset.
_Avoid_: Stock watchlist item, existing holding

**Asset classification**:
The portfolio-analysis taxonomy assigned to an asset, including its top-level asset class and any relevant style, credit, duration, or management attributes.
_Avoid_: Asset type, ticker metadata

**Tracked asset**:
An asset known to rstock and available for transactions or portfolio-relevant analysis, whether or not it is currently held.
_Avoid_: Holding, arbitrary ticker

**Transaction ledger**:
The chronological record of buys, sells, dividends, and splits used to derive holdings and NAV history.
_Avoid_: Data import, portfolio table

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
- A **Market data limitation** is part of the result of preparing market data when the limitation is user-actionable, not only diagnostic logging.
- A **Market data source** supplies raw observations; rstock decides whether they become **Historical market data**, **Live quote**, or **Portfolio-relevant analysis** inputs.
- Price lookup identity is part of market data: stocks use ticker; funds and ETFs use Morningstar code.
- User-facing **Tracked asset** identity is ticker for stocks and ISIN for funds/ETFs.
- A **Tracked asset** keeps the same user-facing identity, vehicle type, and currency after creation; descriptive classification and provider lookup metadata may be corrected.
- A held fund or ETF without a Morningstar code cannot provide required **Historical market data** for **NAV**.
- A fund or ETF **Tracked asset** requires a Morningstar code at creation/import time; stocks do not.
- Benchmark market data follows the same historical availability rules as holdings, but a benchmark is not a holding.
- Market data can be represented as a native asset price, an FX rate, and a EUR valuation price for valuation and audit.
- **Forward-filled market data** is allowed only between source observations and never beyond the last date returned by the source.
- The **Base currency** has an implicit FX rate of 1.0.
- A **Market data limitation** for FX is described by the non-**Base currency** that could not support conversion, not by a provider-specific currency pair string.
- **Acceptable Morningstar lag** affects whether a **Market data limitation** is returned, not **NAV** calculation.
- Stock and FX stale-data warnings are based on **Completed weekday** cadence, not exchange-specific holiday calendars.
- CLI features should support portfolio ledger maintenance, **Portfolio-relevant analysis**, or market data needed for portfolio valuation.
- A **Fund candidate** can be analyzed by Morningstar code without first becoming a portfolio asset.
- Assets entering the portfolio ledger should have **Asset classification** available at creation time, including when created by import.
- **Asset classification** attributes should be consistent with the top-level asset class; equity-specific attributes belong to equity assets, and fixed-income-specific attributes belong to fixed-income assets.
- A **Tracked asset** may exist before, during, or after it is held in the portfolio.
- Rolling correlation analysis compares **Tracked assets**, not arbitrary market symbols.
- Correlation analysis uses aligned available **Base currency** series for each **Tracked asset** and benchmark; it does not force every series to one **Effective valuation date**.
- The **Transaction ledger** is the source of truth for holdings and transaction CSV import/export.
- **Transaction ledger** entries use positive quantities, prices, dividend amounts, and split ratios; fees are non-negative.
- A dividend transaction records the total cash received for the asset, not the per-share dividend rate.
- A split transaction records the new-units-per-old-unit ratio; the ratio multiplies existing quantity.

## Example Dialogue

> **Dev:** "MSFT has a price for today, but the ETF only has data through yesterday. Should NAV use today's MSFT price?"
> **Domain expert:** "No. NAV uses the effective valuation date where all required prices and FX rates are available; individual prices may still show their own latest quote."

## Flagged Ambiguities

- "latest price" can mean either **Individual price** or the market data limiting the **Effective valuation date**; use the precise term.
- Missing market data for a held asset or required FX rate is not the same as stale market data; missing data stops **NAV** calculation, while stale data may move the **Effective valuation date** earlier.
- "analysis" can mean either **Portfolio-relevant analysis** or general market research; rstock uses the portfolio-relevant meaning unless a separate research feature is explicitly introduced.
- **Asset classification** is not the same as asset type; asset type describes the vehicle, while **Asset classification** describes how the asset contributes to portfolio analysis.
- "asset" can mean a **Tracked asset** or a current holding; use **Tracked asset** when the asset does not need an open position.
