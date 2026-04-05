# Architecture

## System Overview

```
                          ┌──────────────┐
                          │   Terminal    │
                          └──────┬───────┘
                                 │
                          ┌──────┴───────┐
                          │   CLI Layer  │
                          │  cli.rs      │
                          │  main.rs     │
                          └──────┬───────┘
                                 │
                   ┌─────────────┼─────────────┐
                   │             │             │
            ┌──────┴──────┐ ┌───┴────┐ ┌──────┴──────┐
            │  Services   │ │ Models │ │   Display   │
            │  nav.rs     │ │        │ │ display.rs  │
            │  portfolio  │ │ asset  │ │  (tables,   │
            │  prices     │ │ tx     │ │   charts,   │
            │  metrics    │ │ port.  │ │   reports)  │
            │  holdings   │ │ monitor│ │             │
            │  monitor    │ │        │ │             │
            │  export     │ │        │ │             │
            └──────┬──────┘ └────────┘ └─────────────┘
                   │
         ┌─────────┴─────────┐
         │                   │
  ┌──────┴──────┐    ┌──────┴──────┐
  │  DB Layer   │    │PriceFetcher │
  │  repos/     │    │   trait     │
  │  entities/  │    └──────┬──────┘
  └──────┬──────┘           │
         │           ┌──────┴──────┐
  ┌──────┴──────┐    │  External   │
  │   SQLite    │    │ Yahoo Fin.  │
  │ ~/.rstock/  │    │ mstarpy     │
  │ rstock.db   │    │ (Python)    │
  └─────────────┘    └─────────────┘
```

## Layers

### CLI Layer (`cli.rs`, `main.rs`)

`cli.rs` defines ten subcommands using clap 4.5 derive macros:
- **`get`** — Display portfolio with optional `--period` (1m, 3m, 6m, ytd, 1y, 3y, 5y, all)
- **`buy`** — Record a purchase (ticker/ISIN, name, type, date, quantity, price, optional: fees, currency)
- **`sell`** — Record a sale (ticker, date, quantity, price, optional: fees)
- **`dividend`** — Record a dividend payment (ticker, date, amount, optional: fees)
- **`split`** — Record a stock split or reverse split (ticker, date, ratio)
- **`list`** — Show all assets in the portfolio
- **`export`** — Export transactions to CSV (--output path)
- **`holdings`** — Show portfolio holdings breakdown (stocks directly, funds/ETFs with underlying positions)
- **`analyze`** — Portfolio correlation matrix with configurable period (30d, 6m, 1y, 3y, 5y)
- **`monitor`** — Subcommand group: add/remove/list/view stocks in a watchlist with momentum indicators

`main.rs` dispatches commands to service functions. It also handles chart period date-range calculation and date validation (rejects future dates).

### Logging (`logging.rs`)

Structured logging via `tracing`. Two output layers:
- **stderr** — Colored, compact format. Controlled by `-v` flag or `RUST_LOG` env var. Default: warnings only.
- **File** — Daily-rotating log at `~/.rstock/rstock.log`. Full timestamps, includes module target path.

Verbosity mapping: default=WARN, `-v`=INFO, `-vv`=DEBUG, `-vvv`=TRACE.

### Service Layer (`services/`)

All business logic lives here. Key modules:

**`nav.rs`** — Core NAV unitization engine. `rebuild_portfolio_history()` iterates calendar days from a start date to the effective end date, computing daily portfolio snapshots. `process_day_transactions()` handles share issuance (buys) and redemption (sells). `compute_day_asset_values()` calculates end-of-day portfolio value with currency conversion.

**`portfolio.rs`** — `get_portfolio()` builds the current position table (per-asset quantity, avg cost, gain/loss). `get_portfolio_summary()` triggers NAV rebuild if stale, then computes return metrics (YTD simple return, annualized CAGR for 1Y/3Y/5Y).

**`transactions.rs`** — `buy()` records a purchase and invalidates snapshots from the buy date forward. `sell()` validates holdings (cannot sell more than owned), records the sale, and invalidates snapshots. `dividend()` records a dividend payment. `split()` records a stock split, adjusting quantity via the split ratio.

**`daily_prices.rs`** — `fill_prices_for_range()` fetches prices from the API, caches them, and forward-fills gaps for weekends/holidays. Never fills beyond the last API date. `get_closing_price()` is the main lookup function.

**`exchange_rates.rs`** — Same caching pattern as daily_prices. All rates convert to EUR (base currency). Pairs stored as `XXXEUR` format (e.g., `USDEUR`).

**`price.rs`** — Defines the `PriceFetcher` async trait with two methods: `get_historical_prices()` and `get_historical_exchange_rates()`. `RealPriceFetcher` implements it using `yfinance-rs` for stocks and `uv run scripts/get_fund_price_history.py` for funds/ETFs.

**`metrics.rs`** — `compute_risk_metrics()` calculates per-period volatility, max drawdown, beta, and Sharpe ratio against the MSCI ACWI benchmark. `compute_correlation_matrix()` builds an N×N asset correlation matrix from daily returns. Uses daily log returns, 252 trading days/year, 3% annual risk-free rate. Requires at least 20 data points.

**`holdings.rs`** — `get_holdings()` builds a look-through report: stocks are listed directly, while funds/ETFs have their underlying positions fetched via `scripts/get_fund_holdings.py`.

**`monitor.rs`** — `generate_monitor_report()` fetches price history for a stock and its sector ETF, computes momentum indicators (RSI-14, SMA-50/200, MACD), fundamentals, and relative strength metrics.

**`export.rs`** — `export_transactions_csv()` dumps all transactions to a CSV file.

### Display Layer (`display/`)

Pure output formatting with no business logic, split into submodules:

- **`helpers.rs`** — Shared formatting utilities (price, quantity, percentage, color helpers)
- **`portfolio.rs`** — `print_portfolio()` renders per-asset table and summary (NAV, returns, risk metrics) using `tabled` with green/red coloring via `colored`
- **`simple.rs`** — `print_asset_list()` lists all tracked assets, `print_nav_chart()` renders ASCII NAV chart via `textplots`, `print_watchlist()` lists monitored stocks
- **`correlation.rs`** — `print_correlation_matrix()` renders N×N correlation matrix with color-coded values
- **`holdings.rs`** — `print_holdings()` renders fund/ETF look-through report with underlying positions
- **`monitor.rs`** — `print_monitor_report()` renders stock analysis with fundamentals, momentum indicators, and sector comparison charts

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
- `PortfolioSummary` — Statistics (nav, daily change, period returns, per-period PeriodMetrics)
- `PeriodMetrics` — Per-period volatility, max drawdown, beta, and Sharpe ratio
- `PortfolioRow` — Display struct with `tabled::Tabled` derive
- `CorrelationMatrix` — N×N asset correlation matrix with labels and warnings
- `HoldingsResult`, `DirectHolding`, `FundWithHoldings`, `FundHolding` — Look-through holdings models

**`monitor.rs`**:
- `StockInfo` — Fundamentals (price, P/E, EPS, dividend yield, market cap, sector, etc.)
- `MomentumIndicators` — RSI-14, SMA-50/200, MACD with signal interpretations
- `RelationshipMetrics` — Relative strength, beta vs sector, correlation
- `MonitorReport` — Combined report with stock/sector momentum and price series

### DB Layer (`db/`)

**`mod.rs`** — `connect()` creates the SQLite connection at `~/.rstock/rstock.db`, auto-creates parent directories, and runs pending migrations via `sea_orm_migration::MigratorTrait::up()`.

**`entities/`** — SeaORM-generated model structs for each table (8 entities: asset, transaction, daily_asset_price, portfolio_history, portfolio_asset_history, daily_exchange_rate, watchlist). Each entity defines `Model`, `ActiveModel`, `Column`, and `Relation` types.

**`repos/`** — Repository pattern with one module per entity (including `watchlist_repo`). All functions are `async` and take `&DatabaseConnection` as first parameter. Standard function naming: `find_*`, `find_by_*`, `find_*_between`, `find_at_or_before`, `upsert`, `insert_*`, `delete_*`.

### Constants (`constants.rs`)

Centralized constants: `BASE_CURRENCY`, `INITIAL_NAV`, period durations, benchmark configuration (`ACWI`), risk-free rate, trading days, momentum indicator parameters (RSI, SMA, MACD), and floating-point thresholds.

### Utilities (`utils.rs`)

`resolve_scripts_dir()` locates the Python scripts directory (checks `RSTOCK_SCRIPTS_DIR` env var, then walks up from executable).

## Database Schema

Eight tables across five migrations:

### Core Tables (Migration 1)

**`assets`** — Asset definitions
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| ticker | String | UNIQUE — ticker symbol (stocks) or ISIN (funds/ETFs) |
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
| pair | String | e.g., "USDEUR" |
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
Yahoo Finance API ──(yfinance-rs)──> Vec<(date, f64)> ──> daily_asset_prices table
```

`RealPriceFetcher` uses `yfinance_rs::YahooFinance` with `HistoryBuilder` to fetch daily OHLCV data. Only the close price is extracted.

### Fund/ETF Prices

```
Morningstar ──(mstarpy via Python)──> JSON ──> daily_asset_prices table
```

The fetcher runs `uv run scripts/get_fund_price_history.py <ticker> <start> <end>` as a subprocess. For funds/ETFs, the ticker field contains the ISIN. The script uses Python `mstarpy` and outputs `[{"date": "YYYY-MM-DD", "price": f64}, ...]`.

Script resolution order:
1. `RSTOCK_SCRIPTS_DIR` environment variable
2. Walk up from the executable's directory looking for a `scripts/` folder

### Exchange Rates

```
Yahoo Finance ──(yfinance-rs)──> "XXXEUR=X" pairs ──> daily_exchange_rates table
```

Same mechanism as stock prices but queries currency pair symbols.

### Forward-Fill

After caching API responses, gaps between dates (weekends, holidays) are filled with the last known price. The fill never extends beyond the last date returned by the API.

## Data Flow by Command

### `get`

```
main.rs
  └─> portfolio::get_portfolio_summary()
        ├─> Check if NAV snapshots are stale (last snapshot < yesterday)
        ├─> If stale: nav::rebuild_portfolio_history()
        │     ├─> daily_prices::fill_prices_for_range() (for each asset)
        │     ├─> exchange_rates::fill_rates_for_range() (for each currency)
        │     └─> Day-by-day NAV computation loop
        ├─> Compute return metrics (YTD, 1Y, 3Y, 5Y)
        └─> metrics::compute_risk_metrics() (beta, Sharpe)
  └─> portfolio::get_portfolio()
        ├─> Load latest portfolio_asset_history
        ├─> For each position: compute avg cost, gain/loss
        └─> Aggregate totals
  └─> display::print_portfolio_table()
  └─> display::print_portfolio_summary()
  └─> display::print_nav_chart()
```

### `buy`

```
main.rs
  └─> transactions::buy()
        ├─> asset_repo::get_or_create() (upsert asset metadata)
        ├─> transaction_repo::insert_buy() (store with cents conversion)
        └─> portfolio_history_repo::delete_from_date() (invalidate snapshots)
```

### `sell`

```
main.rs
  └─> transactions::sell()
        ├─> asset_repo::find_by_ticker() (must exist)
        ├─> transaction_repo::find_by_asset_id() (load all txs)
        ├─> Validate: net holdings >= sell quantity at sell date
        ├─> transaction_repo::insert_sell() (store with cents conversion)
        └─> portfolio_history_repo::delete_from_date() (invalidate snapshots)
```

### `dividend`

```
main.rs
  └─> transactions::dividend()
        ├─> asset_repo::find_by_ticker() (must exist)
        ├─> transaction_repo::insert_dividend()
        └─> portfolio_history_repo::delete_from_date() (invalidate snapshots)
```

### `split`

```
main.rs
  └─> transactions::split()
        ├─> asset_repo::find_by_ticker() (must exist)
        ├─> transaction_repo::insert_split()
        └─> portfolio_history_repo::delete_from_date() (invalidate snapshots)
```

### `list`

```
main.rs
  └─> asset_repo::find_all()
  └─> display::print_asset_list()
```

### `export`

```
main.rs
  └─> export::export_transactions_csv()
        ├─> transaction_repo::find_all() (load all txs with asset info)
        └─> Write CSV file
```

### `holdings`

```
main.rs
  └─> holdings::get_holdings()
        ├─> portfolio::get_portfolio() (current positions)
        ├─> For stocks: list directly
        └─> For funds/ETFs: run scripts/get_fund_holdings.py (mstarpy)
  └─> display::print_holdings()
```

### `analyze portfolio`

```
main.rs
  └─> metrics::compute_correlation_matrix()
        ├─> portfolio_asset_history_repo (load daily returns per asset)
        ├─> Compute pairwise Pearson correlations
        └─> Return N×N matrix
  └─> display::print_correlation_matrix()
```

### `monitor view`

```
main.rs
  └─> watchlist_repo::find_by_ticker() (must be in watchlist)
  └─> monitor::generate_monitor_report()
        ├─> fetcher.get_historical_prices() (stock + sector ETF)
        ├─> Compute momentum indicators (RSI, SMA, MACD)
        ├─> Fetch fundamentals via yfinance-rs
        └─> Compute relative strength and correlation
  └─> display::print_monitor_report()
```
