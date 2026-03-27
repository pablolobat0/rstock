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
            │  prices     │ │ tx     │ │   charts)   │
            │  metrics    │ │ port.  │ │             │
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

`cli.rs` defines three subcommands using clap 4.5 derive macros:
- **`get`** — Display portfolio with optional `--period` (ytd, 1y, 3y, 5y, all)
- **`buy`** — Record a purchase (ticker, name, type, date, quantity, price, optional: isin, fees, currency, notes)
- **`sell`** — Record a sale (ticker, date, quantity, price, optional: fees, notes)

`main.rs` dispatches commands to service functions. It also handles chart period date-range calculation and date validation (rejects future dates).

### Service Layer (`services/`)

All business logic lives here. Key modules:

**`nav.rs`** — Core NAV unitization engine. `rebuild_portfolio_history()` iterates calendar days from a start date to the effective end date, computing daily portfolio snapshots. `process_day_transactions()` handles share issuance (buys) and redemption (sells). `compute_day_asset_values()` calculates end-of-day portfolio value with currency conversion.

**`portfolio.rs`** — `get_portfolio()` builds the current position table (per-asset quantity, avg cost, gain/loss). `get_portfolio_summary()` triggers NAV rebuild if stale, then computes return metrics (YTD simple return, annualized CAGR for 1Y/3Y/5Y).

**`transactions.rs`** — `buy()` records a purchase and invalidates snapshots from the buy date forward. `sell()` validates holdings (cannot sell more than owned), records the sale, and invalidates snapshots.

**`daily_prices.rs`** — `fill_prices_for_range()` fetches prices from the API, caches them, and forward-fills gaps for weekends/holidays. Never fills beyond the last API date. `get_closing_price()` is the main lookup function.

**`exchange_rates.rs`** — Same caching pattern as daily_prices. All rates convert to EUR (base currency). Pairs stored as `XXXEUR` format (e.g., `USDEUR`).

**`price.rs`** — Defines the `PriceFetcher` async trait with two methods: `get_historical_prices()` and `get_historical_exchange_rates()`. `RealPriceFetcher` implements it using `yfinance-rs` for stocks and `uv run scripts/get_fund_price_history.py` for funds/ETFs.

**`metrics.rs`** — `compute_risk_metrics()` calculates portfolio beta and Sharpe ratio against the MSCI ACWI benchmark. Uses daily log returns, 252 trading days/year, 3% annual risk-free rate. Requires at least 20 data points.

### Display Layer (`display.rs`)

Pure output formatting with no business logic:
- `print_portfolio_table()` — Renders per-asset table using `tabled` with green/red coloring via `colored`
- `print_portfolio_summary()` — Renders NAV, daily change, period returns, beta, Sharpe
- `print_nav_chart()` — ASCII NAV chart via `textplots`
- Helper functions: `format_qty()`, `color_value()`, `format_return()`

### Model Layer (`models/`)

Domain structs organized by concept:

**`asset.rs`**:
- `AssetType` enum — Stock, Fund, Etf (derives `ValueEnum` for clap integration)
- `AssetInfo` — Input struct for creating/updating assets
- `Asset` — DB-backed struct with id (converted from `asset::Model`)
- `AssetPosition` — Display-ready holding with current price, gain/loss, avg cost

**`transaction.rs`**:
- `BuyOrder`, `SellOrder` — Input structs for recording transactions
- `Transaction` — DB-backed struct with `price_cents` and `fees_cents` (i64)
- `f64_to_cents()` / `cents_to_f64()` — Conversion helpers for monetary precision

**`portfolio.rs`**:
- `PortfolioSnapshot` — Daily NAV snapshot (date, asset_value, total_value, outstanding_shares, nav)
- `AssetSnapshot` — Per-asset daily position (quantity, closing_price, market_value, exchange_rate)
- `PortfolioResult` — Query result with asset rows and aggregated totals
- `PortfolioSummary` — Statistics (nav, daily change, period returns, beta, sharpe)
- `PortfolioRow` — Display struct with `tabled::Tabled` derive

### DB Layer (`db/`)

**`mod.rs`** — `connect()` creates the SQLite connection at `~/.rstock/rstock.db`, auto-creates parent directories, and runs pending migrations via `sea_orm_migration::MigratorTrait::up()`.

**`entities/`** — SeaORM-generated model structs for each table. Each entity defines `Model`, `ActiveModel`, `Column`, and `Relation` types.

**`repos/`** — Repository pattern with one module per entity. All functions are `async` and take `&DatabaseConnection` as first parameter. Standard function naming: `find_*`, `find_by_*`, `find_*_between`, `find_at_or_before`, `upsert`, `insert_*`, `delete_*`.

## Database Schema

Seven tables across four migrations:

### Core Tables (Migration 1)

**`assets`** — Asset definitions
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| ticker | String | UNIQUE |
| isin | String? | Optional ISIN code |
| name | String | Full asset name |
| asset_type | String | "stock", "fund", or "etf" |
| currency | String | Asset's native currency |
| created_at | String | ISO timestamp |

**`transactions`** — Buy/sell records
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| asset_id | i32 FK | References assets.id |
| tx_type | String | "buy" or "sell" |
| date | String | YYYY-MM-DD |
| quantity | f64 | Number of shares/units |
| price_cents | i64 | Price per unit in cents |
| fees_cents | i64 | Commission in cents |
| notes | String? | Optional |
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

### Relationships

```
assets 1──* transactions
assets 1──* daily_asset_prices
assets 1──* portfolio_asset_history
```

`portfolio_history` and `daily_exchange_rates` are standalone (no foreign keys to assets).

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

The fetcher runs `uv run scripts/get_fund_price_history.py <isin> <start> <end>` as a subprocess. The script uses Python `mstarpy` and outputs `[{"date": "YYYY-MM-DD", "price": f64}, ...]`.

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
