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
Non-persisted same-day asset price or FX rate used for current display before the market day is complete. Stocks may supply one, ETFs may supply one through a capable Market data source, and mutual funds do not.
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

**Fund candidate correlation**:
The relationship between a Fund candidate's daily returns and the daily returns of the portfolio NAV or a currently held Tracked asset.
_Avoid_: Holdings overlap, benchmark beta

**Common fund holding**:
A security that appears in the reported holdings of both funds being compared, shown with each fund's own holding weight.
_Avoid_: Portfolio exposure, combined allocation

**Fund quote metadata**:
Current fund-level facts used in fund analysis, including fund name, assets under management, and inception date.
_Avoid_: Holdings, price history

**Asset classification**:
The portfolio-analysis taxonomy assigned to an asset, including its top-level asset class and any relevant style, credit, duration, or management attributes.
_Avoid_: Asset type, ticker metadata

**Tracked asset**:
An asset known to rstock and available for transactions or portfolio-relevant analysis, whether or not it is currently held.
_Avoid_: Holding, arbitrary ticker

**Transaction ledger**:
The chronological record of buys, sells, dividends, and splits used to derive holdings and NAV history.
_Avoid_: Data import, portfolio table

**Monetary holding**:
A currently held Tracked asset with the Monetary Asset classification, shown as portfolio inventory but excluded from portfolio performance measurement.
_Avoid_: Cash balance, performance asset

**Portfolio view**:
The user-facing view of current Transaction ledger inventory and its latest available Individual prices, shown alongside NAV and returns at their Effective valuation date.
_Avoid_: NAV snapshot, current NAV valuation

**Average cost**:
The weighted-average Base currency acquisition cost per currently held unit, including buy fees; sells remove cost proportionally and splits change units without changing total cost.
_Avoid_: Tax basis, FIFO cost

**Open-position gain/loss**:
The difference between a currently held position's current Base currency value and its remaining weighted-average cost; it excludes dividends and realized gains from sold units.
_Avoid_: Total return, lifetime gain/loss

## Relationships

- **NAV** is calculated at one **Effective valuation date**.
- An **Effective valuation date** is constrained by the oldest latest-available market data needed across all holdings and FX rates.
- An **Individual price** may be newer than the **Effective valuation date**.
- **NAV** cannot be calculated when any held asset or required FX rate has no market data for the valuation period.
- **Stale market data** may move the **Effective valuation date** earlier.
- **NAV** uses **Historical market data**, not **Live quote** values.
- An **Individual price** for a stock may use a **Live quote** for display.
- Mutual funds use a single closing price and do not use **Live quote** values for display.
- An ETF **Individual price** requests a same-day observation through the existing fund-price capability of `MarketDataSources`; when the configured **Market data source** supplies one it is a **Live quote**, otherwise the ETF uses the latest available **Historical market data**.
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
- Transaction ledger cost and dividend facts in the **Base currency** use the latest FX rate on or before each transaction date; when no such rate exists, those facts and dependent gain/loss facts are unavailable rather than estimated with a current or later FX rate.
- A **Market data limitation** for FX is described by the non-**Base currency** that could not support conversion, not by a provider-specific currency pair string.
- **Acceptable Morningstar lag** affects whether a **Market data limitation** is returned, not **NAV** calculation.
- Stock and FX stale-data warnings are based on **Completed weekday** cadence, not exchange-specific holiday calendars.
- CLI features should support portfolio ledger maintenance, **Portfolio-relevant analysis**, or market data needed for portfolio valuation.
- A **Fund candidate** can be analyzed by Morningstar code without first becoming a portfolio asset.
- **Fund candidate correlation** can compare a **Fund candidate** with the whole portfolio through **NAV**, and with each currently held **Tracked asset** individually.
- A **Common fund holding** is based only on the two compared funds' reported holdings, not the user's current portfolio exposure.
- **Fund quote metadata** is displayed at analysis time and is not part of the transaction ledger or NAV history.
- Fund analysis can run for a **Fund candidate** that is not a **Tracked asset**; when no local asset name exists, the fund name from **Fund quote metadata** is used.
- In fund comparison, standard performance metrics use YTD, 1Y, 3Y, 5Y, and all-time periods, while the selected period controls only the correlation and aligned return graph.
- **Fund candidate correlation** is omitted for the portfolio NAV or any held **Tracked asset** that does not have return coverage for the requested correlation period.
- Fund comparison correlation and aligned return graph require full coverage for the selected period; they do not fall back to shorter overlapping history.
- Fund comparison updates holdings snapshot history for each compared fund but does not show holdings snapshot diffs.
- Assets entering the portfolio ledger should have **Asset classification** available at creation time, including when created by import.
- **Asset classification** attributes should be consistent with the top-level asset class; equity-specific attributes belong to equity assets, and fixed-income-specific attributes belong to fixed-income assets.
- A **Tracked asset** may exist before, during, or after it is held in the portfolio.
- A currently held **Tracked asset** remains visible in the portfolio view when no **Individual price** is available; its Transaction ledger quantity and cost facts remain available while price-dependent facts are unavailable.
- One **Transaction ledger** projection derives current quantity, remaining cost, dividends, and **Open-position gain/loss** for both performance and **Monetary holding** positions; classification changes where the position contributes, not how its ledger facts are calculated.
- Position facts are independently available: missing historical FX can make cost or dividend facts unavailable without hiding a known quantity or current value, while a missing **Individual price** makes current value and dependent **Open-position gain/loss** unavailable without hiding ledger facts.
- The portfolio view's current performance-holdings total is unavailable when any currently held performance asset has no **Individual price**; a partial sum must not be presented as the complete total.
- Every aggregate in the **Portfolio view** is either complete across all included holdings or unavailable; known per-holding facts remain visible when an aggregate is unavailable.
- Portfolio composition uses current **Transaction ledger** inventory rather than holdings at the **Effective valuation date**; value-dependent composition is unavailable when any included holding has no **Individual price**.
- Rolling correlation analysis compares **Tracked assets**, not arbitrary market symbols.
- Correlation analysis uses aligned available **Base currency** series for each **Tracked asset** and benchmark; it does not force every series to one **Effective valuation date**.
- The **Transaction ledger** is the source of truth for holdings and transaction CSV import/export.
- **Transaction ledger** entries use positive quantities, prices, dividend amounts, and split ratios; fees are non-negative.
- A dividend transaction records the total cash received for the asset, not the per-share dividend rate.
- A split transaction records the new-units-per-old-unit ratio; the ratio multiplies existing quantity.
- **Average cost** describes current portfolio inventory and does not perform tax-lot or realized-gain accounting.
- A **Monetary holding** is displayed by the portfolio view but is excluded from aggregate portfolio value, allocation weights, gain/loss, NAV, returns, and risk metrics.
- A **Monetary holding** retains its own quantity, Average cost, Individual price, current value, dividends, and Open-position gain/loss for display.
- The portfolio view presents performance holdings as `positions` and **Monetary holding** values separately as `monetary_positions`.
- If a **Monetary holding** has no available Individual price, it remains visible with ledger-derived quantity and cost facts; its current price, price date, value, and gain/loss are unavailable rather than inferred from a transaction price.
- The **Base currency** value of all **Monetary holding** values is reported separately from aggregate portfolio value and is unavailable when any open Monetary holding cannot be valued.
- The portfolio view's Total value is the sum of aggregate portfolio value and the separate Monetary holding value; it is unavailable when either subtotal is unavailable, and it does not participate in portfolio performance measurement.
- The portfolio view's Total value may combine the latest available **Individual price** dates across holdings; it is an informational current estimate, not a synchronized NAV valuation.
- Market data limitations for **Monetary holding** values are reported separately and do not imply a limitation on NAV or portfolio performance.
- The **Portfolio view** reports **Market data limitation** values separately for NAV/history, current performance positions, and **Monetary holding** values; a limitation in one scope does not imply that another scope is invalid.
- Dividends are reported as lifetime income for a **Tracked asset** and are not attributed to the units that remain after a partial sell.
- **Open-position gain/loss** uses only current value and remaining weighted-average cost; dividends and realized gains from sold units are separate facts and never contribute to it.

## Example Dialogue

> **Dev:** "MSFT has a price for today, but the ETF only has data through yesterday. Should NAV use today's MSFT price?"
> **Domain expert:** "No. NAV uses the effective valuation date where all required prices and FX rates are available; individual prices may still show their own latest quote."

## Flagged Ambiguities

- "latest price" can mean either **Individual price** or the market data limiting the **Effective valuation date**; use the precise term.
- Missing market data for a held asset or required FX rate is not the same as stale market data; missing data stops **NAV** calculation, while stale data may move the **Effective valuation date** earlier.
- "analysis" can mean either **Portfolio-relevant analysis** or general market research; rstock uses the portfolio-relevant meaning unless a separate research feature is explicitly introduced.
- **Asset classification** is not the same as asset type; asset type describes the vehicle, while **Asset classification** describes how the asset contributes to portfolio analysis.
- "asset" can mean a **Tracked asset** or a current holding; use **Tracked asset** when the asset does not need an open position.
- Fund beta currently uses the configured benchmark; future work may allow asset-specific or fund-specific benchmark selection.
