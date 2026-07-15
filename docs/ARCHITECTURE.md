# Architecture

## System Overview

```
                          ┌──────────────┐
                          │   Terminal    │
                          └──────┬───────┘
                                 │
                          ┌──────┴───────┐
                          │   CLI Layer  │
                          │ src/cli      │
                          │  main.rs     │
                          └──────┬───────┘
                                 │
                   ┌─────────────┼─────────────┐
                   │             │             │
            ┌──────┴──────┐ ┌───┴────┐ ┌──────┴──────┐
            │  Services   │ │ Models │ │ CLI Output  │
            │  nav.rs     │ │        │ │ src/cli/    │
            │  portfolio  │ │ asset  │ │  (tables,   │
            │  prices     │ │ tx     │ │   charts,   │
            │  metrics    │ │ port.  │ │   reports,  │
            │ composition │ │ monitor│ │             │
            │ fund_analysis││        │ │             │
            │  monitor    │ │        │ │             │
            │  export     │ │        │ │   or JSON)  │
            └──────┬──────┘ └────────┘ └─────────────┘
                   │
         ┌─────────┴─────────┐
         │                   │
  ┌──────┴──────┐    ┌──────┴──────┐
  │  DB Layer   │    │ MarketData  │
  │  repos/     │    │   module    │
  │  entities/  │    └──────┬──────┘
  └──────┬──────┘           │
         │           ┌──────┴──────┐
  ┌──────┴──────┐    │  External   │
  │   SQLite    │    │ Yahoo Fin.  │
  │ ~/.rstock/  │    │ Yahoo Fin.  │
  │ rstock.db   │    │ Morningstar │
  └─────────────┘    └─────────────┘
```

## Layers

### CLI Layer (`src/cli`, `main.rs`)

`src/cli/mod.rs` defines the root commands and grouped subcommands using clap derive macros:
- **`get`** — Display portfolio with optional `--period` (1m, 3m, 6m, ytd, 1y, 3y, 5y, all)
- **`portfolio`** — Grouped dashboard and Tracked asset add/edit commands
- **`transaction`** — List, record, edit, delete, import, or export Transaction ledger entries
- **`analyze`** — Composition, fund analysis, static correlation matrix, and rolling pair correlation
- **`compare`** — Side-by-side fund candidate comparison

`main.rs` creates an `OutputFormat` from the global `--json` flag and passes it through every dispatch path. Command adapters in `src/cli/commands/` call presentation-neutral services, then choose either the existing human renderer or `output::emit_json()`. `src/cli/output.rs` owns compact serialization and emits one `command`/`data` envelope to stdout; services do not emit successful command output. Errors and Clap help/version remain outside this successful-output boundary.

### Logging (`logging.rs`)

Structured logging via `tracing`. Two output layers:
- **stderr** — Colored, compact format. Controlled by `-v` flag or `RUST_LOG` env var. Default: warnings only.
- **File** — Daily-rotating log at `~/.rstock/rstock.log`. Full timestamps, includes module target path.

Verbosity mapping: default=WARN, `-v`=INFO, `-vv`=DEBUG, `-vvv`=TRACE.

### Service Layer (`services/`)

All business logic lives here. Key modules:

**`nav.rs`** — Core NAV unitization engine. `rebuild_portfolio_history()` iterates calendar days from a start date to the effective end date, computing daily portfolio snapshots. `process_day_transactions()` handles share issuance (buys) and redemption (sells). `compute_day_asset_values()` calculates end-of-day portfolio value through strict Historical market data valuation reads.

**`market_data/historical.rs`** — Private implementation for reproducible Historical market data used by NAV and benchmark analytics. It fetches and caches required asset prices, infers required FX from supplied assets, hides provider-specific FX pair construction from external callers, calculates the Effective valuation date, returns actionable Market data limitation values, and exposes strict valuation reads through the `market_data` Module root.

**`market_data/individual_price.rs`** — Private implementation for display-time Individual price values for portfolio rows. Stocks and FX may use non-persisted Live quote values, funds and ETFs use cached Historical market data semantics, and snapshot fallback preserves row rendering when current display data is unavailable.

**`portfolio.rs`** — `get_portfolio()` builds the current position table (per-asset quantity, avg cost, gain/loss) and computes return metrics for the portfolio view. Portfolio rows use the `market_data` Module root for Individual price values and carry Market data limitation values to display formatting.

**`analytics.rs`** — Computes correlation and risk-metric inputs from portfolio history and benchmark prices.

**`composition.rs`** — Builds portfolio composition analytics with look-through aggregation and top holdings.

**`transactions.rs`** — `buy()` records a purchase and invalidates snapshots from the buy date forward. `sell()` validates holdings (cannot sell more than owned), records the sale, and invalidates snapshots. `dividend()` records a dividend payment. `split()` records a stock split, adjusting quantity via the split ratio.

**`market_data/`** — Stateful market data Module. It exposes use-case-shaped Interfaces for valuation market data, correlation market data, Individual price, stock info, and fund data. Yahoo Finance and Morningstar source Adapters are private implementation details behind `MarketDataSources`.

**`metrics.rs`** — Shared math helpers for volatility, max drawdown, Sharpe, Sortino, beta, Pearson correlation, log returns, return alignment, and CAGR. Uses daily log returns, 252 trading days/year, 3% annual risk-free rate, and actual elapsed dates for CAGR.

**`fund_analysis.rs`** — `compute_fund_analysis()` builds a deep-dive report for any Morningstar fund code, including performance, holdings, equity-only allocation tables, and holdings snapshot diffs. Snapshot persistence is keyed by Morningstar's reported portfolio date, so repeated runs against the same Morningstar snapshot do not create duplicate history rows.

**`holdings.rs`** — Shared holdings fetch helper used by composition and other look-through paths.

**`monitor.rs`** — `generate_monitor_report()` fetches price history for a stock and its sector ETF, computes momentum indicators (RSI-14, SMA-50/200, MACD), fundamentals, and relative strength metrics.

**`export.rs`** — `export_transactions_csv()` dumps all transactions to a CSV file.

### Output Layer (`src/cli/display/`, `src/cli/output.rs`)

Pure output formatting with no business logic. `OutputFormat` selects human or JSON output at the CLI boundary. Human formatting remains split into command-oriented display submodules, while `output.rs` provides the shared JSON envelope writer:

- **`helpers.rs`** — Shared formatting utilities (price, quantity, percentage, color helpers)
- **`portfolio.rs`** — `print_portfolio()` renders per-asset table, summary (NAV, returns, risk metrics), and user-facing Market data limitation warning text using `tabled` with green/red coloring via `colored`
- **`simple.rs`** — `print_nav_chart()` renders the ASCII NAV chart via `textplots`
- **`correlation.rs`** — `print_correlation_matrix()` renders N×N correlation matrix with color-coded values
- **`composition.rs`** — `print_composition()` renders composition breakdowns and top holdings
- **`fund_analysis.rs`** — `print_fund_analysis()` renders deep-dive fund analysis tables and snapshot diffs
- **`fund_comparison.rs`** — `print_fund_comparison()` renders side-by-side fund candidate analysis
- **`monitor.rs`** — `print_monitor_report()` renders stock analysis with fundamentals, momentum indicators, and sector comparison charts
- **`output.rs`** — `OutputFormat`, compact `command`/`data` envelope serialization, and stdout emission

### Model Layer (`models/`)

Domain structs organized by concept:

**`asset.rs`**:
- `AssetType` enum — Stock, Fund, Etf (derives `ValueEnum` for clap integration)
- `AssetInfo` — Input struct for creating/updating assets
- `Asset` — DB-backed struct with id (converted from `asset::Model`)
- `AssetPosition` — Display-ready holding with current price, gain/loss, avg cost

**`transaction.rs`**:
- `TxType` enum — Buy, Sell, Dividend, Split (with `Display`/`FromStr` for DB serialization)
- `BuyOrder`, `SellOrder`, `DividendOrder`, `SplitOrder` — Input structs for recording transactions
- `Transaction` — DB-backed struct with `price_cents` and `fees_cents` (i64), includes `compute_holdings()` for net position calculation accounting for splits
- `f64_to_cents()` / `cents_to_f64()` — Conversion helpers for monetary precision

**`portfolio.rs`**:
- `PortfolioSnapshot` — Daily NAV snapshot (date, asset_value, total_value, outstanding_shares, nav)
- `AssetSnapshot` — Per-asset daily position (quantity, closing_price, market_value, exchange_rate)
- `PortfolioResult` — Query result with asset rows and aggregated totals (including total_dividends)
- `PeriodMetrics` — Per-period volatility, max drawdown, beta, Sharpe ratio, and Sortino ratio
- `CorrelationMatrix` — N×N asset correlation matrix with labels and warnings
- `AllocationEntry`, `TopHolding`, `CompositionResult`, `FundHolding` — Composition and holdings models

**`fund_analysis.rs`**:
- `FundAnalysisResult` — Full fund report including top holdings, allocations, period metrics, and holdings snapshot diff
- `FundPeriodMetrics` — Total return, CAGR, volatility, max drawdown, beta, Sharpe, and Sortino
- `HoldingChange`, `HoldingChangeType`, `FundData` — Snapshot diff and Morningstar payload models

**`monitor.rs`**:
- `StockInfo` — Fundamentals (price, P/E, EPS, dividend yield, market cap, sector, etc.)
- `MomentumIndicators` — RSI-14, SMA-50/200, MACD with signal interpretations
- `RelationshipMetrics` — Relative strength, beta vs sector, correlation
- `MonitorReport` — Combined report with stock/sector momentum and price series

### DB Layer (`db/`)

**`mod.rs`** — `connect()` creates the SQLite connection at `~/.rstock/rstock.db`, auto-creates parent directories, and runs pending migrations via `sea_orm_migration::MigratorTrait::up()`.

**`entities/`** — SeaORM-generated model structs for each table, including `fund_holdings_snapshot`. Each entity defines `Model`, `ActiveModel`, `Column`, and `Relation` types.

**`repos/`** — Repository pattern with one module per entity (including `watchlist_repo`). All functions are `async` and take `&DatabaseConnection` as first parameter. Standard function naming: `find_*`, `find_by_*`, `find_*_between`, `find_at_or_before`, `upsert`, `insert_*`, `delete_*`.

### Constants (`constants.rs`)

Centralized constants: `BASE_CURRENCY`, `INITIAL_NAV`, period durations, benchmark configuration (`ACWI`), risk-free rate, trading days, momentum indicator parameters (RSI, SMA, MACD), and floating-point thresholds.

### Utilities (`utils.rs`)

`confirm_action()` provides a shared interactive confirmation prompt for destructive user-facing operations.

## Database Schema

Current schema includes the original portfolio tables plus watchlist and fund holdings snapshots.

### Core Tables (Migration 1)

**`assets`** — Asset definitions
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| ticker | String | UNIQUE — ticker symbol or user-facing asset identifier |
| name | String | Full asset name |
| asset_type | String | "stock", "fund", or "etf" |
| currency | String | Asset's native currency |
| created_at | String | ISO timestamp |

**`transactions`** — Buy/sell records
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| asset_id | i32 FK | References assets.id |
| tx_type | String | "buy", "sell", "dividend", or "split" |
| date | String | YYYY-MM-DD |
| quantity | f64 | Number of shares/units |
| price_cents | i64 | Price per unit in cents |
| fees_cents | i64 | Commission in cents |
| created_at | String | ISO timestamp |

### Price Cache (Migration 2)

**`daily_asset_prices`** — Cached closing prices
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| asset_id | i32 FK | References assets.id |
| date | String | YYYY-MM-DD, UNIQUE with asset_id |
| closing_price | f64 | Price in asset's native currency |
| is_api_failure | bool | True if API lookup failed |

**`portfolio_history`** — Daily NAV snapshots
| Column | Type | Notes |
|--------|------|-------|
| date | String PK | YYYY-MM-DD |
| cash_balance | f64 | (Reserved, currently unused) |
| asset_value | f64 | Total market value in EUR |
| total_value | f64 | Total portfolio value in EUR |
| outstanding_shares | f64 | NAV shares outstanding |
| nav | f64 | NAV per share |

### Per-Asset History (Migration 3)

**`portfolio_asset_history`** — Daily per-asset positions
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| date | String | YYYY-MM-DD |
| asset_id | i32 FK | References assets.id |
| quantity | f64 | Shares held |
| closing_price | f64 | Price in native currency |
| market_value | f64 | Value in EUR |
| exchange_rate | f64 | FX rate used |

### Exchange Rates (Migration 4)

**`daily_exchange_rates`** — Cached FX rates
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| from_currency | String | Source currency, e.g., "USD" |
| to_currency | String | Target currency, e.g., "EUR" |
| date | String | YYYY-MM-DD |
| rate | f64 | Conversion rate |

### Watchlist (Migration 5)

**`watchlist`** — Monitored stocks for analysis
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| ticker | String | UNIQUE |
| sector_etf_ticker | String | Sector ETF for comparison |
| created_at | String | ISO timestamp |

### Relationships

```
assets 1──* transactions
assets 1──* daily_asset_prices
assets 1──* portfolio_asset_history
```

`portfolio_history`, `daily_exchange_rates`, and `watchlist` are standalone (no foreign keys to assets).

## NAV Unitization Algorithm

The NAV engine (`src/services/nav.rs`) uses the same valuation method as mutual funds:

1. **Initialization**: The day before the first transaction, a seed snapshot is created with NAV = 100.0 and zero shares.

2. **Daily iteration**: For each calendar day from start to effective end:

   a. **Process transactions for this day**:
      - **Buy (deposit)**: If it's the first deposit (outstanding_shares == 0), set NAV = 100.0 and issue `deposit_amount / 100.0` shares. Otherwise, issue `deposit_amount / current_nav` new shares at the current NAV.
      - **Sell (withdrawal)**: Calculate proceeds, redeem `proceeds / current_nav` shares, reducing outstanding shares.

   b. **Compute end-of-day values**: For each held asset, look up closing price and exchange rate, calculate `quantity * price * fx_rate`. Sum all positions for total portfolio value.

   c. **Calculate NAV**: `nav = total_value / outstanding_shares`

   d. **Store snapshot**: Write the day's `portfolio_history` and `portfolio_asset_history` records.

3. **Effective end date**: The rebuild never extends beyond yesterday, and further limits to the earliest "last available date" across all assets' prices and exchange rates. This prevents extrapolation when data sources lag.

4. **Incremental rebuild**: On subsequent runs, the engine starts from the day after the last known snapshot (or rebuilds from a given date when snapshots are invalidated by new transactions).

## NAV Return vs G/L%

The `get` command displays two return figures that can differ:

- **G/L% (money-weighted)**: Simple `(current_value - total_invested) / total_invested`. Treats all deposits equally regardless of timing.
- **NAV return (time-weighted)**: Measures portfolio unit performance from inception (NAV 100.0). Unaffected by deposit timing because each deposit issues new shares at the current NAV.

These diverge whenever deposits are made at different NAVs (e.g., DCA over time). If you add money after the portfolio has already gained, G/L% is diluted by the higher average cost, while NAV return is not — it reflects investment skill independent of cash flow timing. This is the same distinction mutual funds use: NAV return measures how the fund performed, not how much a specific investor gained.

Example: a first deposit at NAV 100 and a second deposit at NAV 129 will show a higher NAV return than G/L%, because G/L% blends the returns of both deposits while NAV tracks per-unit growth from inception.

## Price Data Pipeline

### Stock Prices

```
Yahoo Finance Adapter ──> SourceObservation ──> MarketData ──> daily_asset_prices table
```

The private Yahoo adapter uses `yfinance_rs::YahooFinance` with `HistoryBuilder` to fetch daily OHLCV data. Only the close price is extracted.

### Fund/ETF Prices

```
Morningstar Adapter ──> SourceObservation ──> MarketData ──> daily_asset_prices table
```

The private Morningstar adapter fetches fund/ETF price history using the stored Morningstar code and returns source-neutral observations to `MarketData`.

### Exchange Rates

```
Yahoo Finance ──(yfinance-rs)──> source-neutral currency columns ──> daily_exchange_rates table
```

Same mechanism as stock prices. Provider-specific Yahoo pair symbols are constructed at fetch time and are not stored in the database.

Correlation analytics use cache-first Base currency series prepared by `MarketData`.

### Forward-Fill

Historical market data preparation caches source observations and fills gaps between dates (weekends, holidays) with the last known price or FX rate. The fill never extends beyond the last date returned by the source.

## Data Flow by Command

### `get`

```
main.rs
  └─> portfolio::get_portfolio()
        ├─> Check if NAV snapshots are stale (last snapshot < yesterday)
        ├─> If stale: nav::rebuild_portfolio_history()
        │     ├─> market_data.prepare_valuation_market_data()
        │     └─> Day-by-day NAV computation loop using strict valuation reads
        ├─> Compute return metrics (YTD, 1Y, 3Y, 5Y)
        └─> metrics::compute_risk_metrics() (beta, Sharpe, Sortino)
  └─> portfolio::get_portfolio()
        ├─> Load latest portfolio_asset_history
        ├─> For each position: individual_price::get_asset_display_market_data()
        ├─> Compute avg cost, gain/loss
        └─> Aggregate totals and Market data limitation values
  └─> display::print_portfolio()
  └─> display::print_nav_chart()
```

### `transaction buy`

```
main.rs
  └─> transactions::buy()
        ├─> asset_repo::find_by_ticker() (asset must already exist)
        ├─> transaction_repo::insert_buy() (store with cents conversion)
        └─> portfolio_history_repo::delete_from_date() (invalidate snapshots)
```

### `transaction sell`

```
main.rs
  └─> transactions::sell()
        ├─> asset_repo::find_by_ticker() (must exist)
        ├─> transaction_repo::find_by_asset_id() (load all txs)
        ├─> Validate: net holdings >= sell quantity at sell date
        ├─> transaction_repo::insert_sell() (store with cents conversion)
        └─> portfolio_history_repo::delete_from_date() (invalidate snapshots)
```

### `transaction dividend`

```
main.rs
  └─> transactions::dividend()
        ├─> asset_repo::find_by_ticker() (must exist)
        ├─> transaction_repo::insert_dividend()
        └─> portfolio_history_repo::delete_from_date() (invalidate snapshots)
```

### `transaction split`

```
main.rs
  └─> transactions::split()
        ├─> asset_repo::find_by_ticker() (must exist)
        ├─> transaction_repo::insert_split()
        └─> portfolio_history_repo::delete_from_date() (invalidate snapshots)
```

### `transaction export`

```
main.rs
  └─> export::export_transactions_csv()
        ├─> transaction_repo::find_all() (load all txs with asset info)
        └─> Write CSV file
```

### `analyze composition`

```
main.rs
  └─> composition::compute_composition()
        ├─> portfolio::trigger_rebuild_if_needed()
        ├─> Aggregate classifications and look-through holdings
        └─> Return composition result
  └─> display::print_composition()
```

### `analyze correlation matrix`

```
main.rs
  └─> analytics::compute_correlation_data()
        ├─> portfolio_asset_history_repo (load daily returns per asset)
        ├─> Compute pairwise Pearson correlations
        └─> Return N×N matrix
  └─> display::print_correlation_matrix()
```

### `analyze correlation rolling`

```
main.rs
  └─> analytics::compute_rolling_correlation_data()
        ├─> Fetch stock metadata for the two requested tickers
        ├─> Request cache-first tracked correlation market data from MarketData
        ├─> Use Base currency series returned by MarketData
        ├─> metrics::align_return_series_with_dates_unfiltered()
        ├─> metrics::compute_rolling_correlation() over trailing 60-day windows
        └─> Return dated rolling series + summary stats
  └─> display::print_rolling_correlation()
```

### `analyze fund`

```
main.rs
  └─> fund_analysis::compute_fund_analysis()
        ├─> asset_repo::find_by_morningstar_code()
        ├─> market_data.fund_data() for Morningstar fund data
        ├─> MarketData source adapters for fund + benchmark history
        ├─> metrics::compute_cagr(), beta, Sharpe, Sortino, volatility, max drawdown
        ├─> Build trailing 1Y/3Y/5Y windows from trading-day history
        ├─> Filter allocation rows to holdings with ticker
        └─> fund_holdings_snapshot_repo compare/store by Morningstar portfolio date
  └─> display::print_fund_analysis()
```
