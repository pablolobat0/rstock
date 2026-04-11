# rstock

A CLI portfolio tracker with NAV unitization, price fetching, and ASCII charts.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Overview

rstock tracks your investment portfolio from the terminal. It records buy/sell transactions, dividends, and stock splits, fetches live prices from Yahoo Finance and Morningstar, computes daily portfolio NAV using unitization (the same method used by mutual funds), and renders performance charts directly in your terminal.

## Features

- **Transaction recording** — Log buys, sells, dividends, and stock splits for stocks, ETFs, and funds
- **NAV unitization** — Daily portfolio valuation using unit-based accounting (first deposit = NAV 100, subsequent deposits issue shares at previous day's NAV)
- **Multi-source price fetching** — Stocks via Yahoo Finance (`yfinance-rs`), funds/ETFs via Morningstar (`mstarpy`)
- **Price caching** — Historical prices stored in SQLite with forward-fill for weekends/holidays
- **Multi-currency support** — All portfolio values converted to EUR base currency with cached FX rates
- **Performance tracking** — YTD, 1Y, 3Y, 5Y, and all-time returns with per-period volatility, max drawdown, beta, and Sharpe ratio
- **ASCII NAV chart** — Terminal-rendered portfolio performance chart via `textplots`
- **Colored portfolio table** — Per-asset breakdown with gain/loss highlighting and dividend tracking
- **Correlation analysis** — Portfolio asset correlation matrix over configurable periods
- **Stock monitoring** — Watchlist with momentum indicators (RSI, SMA, MACD), fundamentals, and sector comparison
- **Holdings look-through** — Fund/ETF underlying position breakdown via Morningstar
- **Export** — Dump transactions to CSV
- **Asset listing** — View all tracked assets at a glance

## Prerequisites

- **Rust** (edition 2021)
- **SQLite** (bundled via `sqlx-sqlite`)
- **Python 3.10+** and **[uv](https://github.com/astral-sh/uv)** — required only for fund/ETF price fetching (scripts use `mstarpy` via inline PEP 723 metadata)

## Installation

```bash
git clone https://github.com/ploubser/rstock.git
cd rstock
cargo build --release
```

The binary will be at `target/release/rstock`. The database is created automatically at `~/.rstock/rstock.db` on first run.

## Usage

### Record a buy transaction

```bash
rstock buy \
  --ticker MSFT \
  --name "Microsoft" \
  --type stock \
  --date 2026-02-26 \
  --quantity 1 \
  --price 390
```

All `buy` flags:

| Flag         | Required | Default | Description                          |
|--------------|----------|---------|--------------------------------------|
| `--ticker`   | Yes      |         | Ticker symbol                        |
| `--name`     | Yes      |         | Full asset name                      |
| `--type`     | Yes      |         | Asset type: `stock`, `fund`, or `etf`|
| `--date`     | Yes      |         | Purchase date (`YYYY-MM-DD`)         |
| `--quantity` | Yes      |         | Number of shares/units (fractional OK)|
| `--price`    | Yes      |         | Price per unit (e.g. `150.25`)       |
| `--isin`     | No       |         | ISIN code (used for fund/ETF lookups)|
| `--fees`     | No       | `0`     | Commission/fees                      |
| `--currency` | No       | `EUR`   | Currency                             |

`buy` is also available as `rstock transaction buy ...`. The root form is a shortcut.

### Record a sell transaction

```bash
rstock transaction sell --ticker MSFT --date 2026-03-01 --quantity 0.5 --price 400
```

### Record a dividend

```bash
rstock transaction dividend --ticker MSFT --date 2026-03-15 --amount 25.50 --fees 3.80
```

### Record a stock split

```bash
rstock transaction split --ticker MSFT --date 2026-03-20 --ratio 2    # 2:1 split
rstock transaction split --ticker MSFT --date 2026-03-20 --ratio 0.25  # 1:4 reverse split
```

### View portfolio

```bash
rstock get                # Portfolio table + NAV summary + 1Y chart
rstock get --period ytd   # YTD chart
rstock get --period 5y    # 5-year chart
```

`get` is also available as `rstock portfolio get ...`. Chart periods: `1m`, `3m`, `6m`, `ytd`, `1y` (default), `3y`, `5y`, `all`.

### List assets and CSV import/export

```bash
rstock portfolio list                # Show all portfolio assets
rstock data export --output txns.csv # Export transactions to CSV
rstock data import --input txns.csv  # Import transactions from CSV
```

### Holdings look-through

```bash
rstock portfolio holdings    # Stocks directly, funds/ETFs with underlying positions
```

### Analyze correlations

```bash
rstock analyze portfolio              # 1Y asset correlation matrix (default)
rstock analyze portfolio --period 30d # 30-day correlations
```

Periods: `30d`, `6m`, `1y` (default), `3y`, `5y`.

### Monitor stocks

```bash
rstock monitor add --ticker AAPL --sector-etf XLK
rstock monitor list
rstock monitor view AAPL                  # 1Y analysis (default)
rstock monitor view AAPL --period 6m      # 6-month analysis
rstock monitor remove --ticker AAPL
```

## Architecture

```
src/
├── cli/                # Clap CLI: root commands (get, buy shortcuts) + groups (portfolio, transaction, data, analyze, monitor)
├── main.rs             # Entry point, command dispatch
├── constants.rs        # Centralized constants (dates, currency, metrics, thresholds)
├── display.rs          # Terminal output: tables, charts, reports
├── utils.rs            # Utility functions (scripts directory resolution)
├── lib.rs              # Public module exports
├── models/
│   ├── asset.rs        # AssetType enum, AssetInfo, Asset, AssetPosition
│   ├── portfolio.rs    # PortfolioSnapshot, PortfolioSummary, CorrelationMatrix, holdings models
│   ├── transaction.rs  # BuyOrder, SellOrder, DividendOrder, SplitOrder, TxType, Transaction
│   └── monitor.rs      # StockInfo, MomentumIndicators, RelationshipMetrics, MonitorReport
├── services/
│   ├── transactions.rs # Buy/sell/dividend/split recording + snapshot invalidation
│   ├── portfolio.rs    # Portfolio summary and per-asset positions
│   ├── nav.rs          # NAV unitization engine
│   ├── daily_prices.rs # Price caching with forward-fill
│   ├── exchange_rates.rs # FX rate caching (EUR base)
│   ├── price.rs        # PriceFetcher trait + Real/Mock implementations
│   ├── metrics.rs      # Beta, Sharpe, volatility, drawdown, correlation matrix
│   ├── holdings.rs     # Fund/ETF look-through holdings
│   ├── monitor.rs      # Stock analysis with momentum indicators
│   └── export.rs       # Transaction CSV export
├── db/
│   ├── mod.rs          # SQLite connection + auto-migration
│   ├── entities/       # SeaORM generated entities (8 tables)
│   └── repos/          # Repository layer (one per entity)
scripts/
├── get_fund_price.py          # Latest fund/ETF NAV (mstarpy)
├── get_fund_price_history.py  # Historical fund/ETF NAV (mstarpy)
└── get_fund_holdings.py       # Fund/ETF underlying holdings (mstarpy)
migration/src/                 # SeaORM migrations (5 migrations)
tests/                         # Integration tests + common test utilities
```

### Data flow

1. `cli.rs` parses commands via clap derive macros
2. `main.rs` dispatches to service functions
3. Services fetch prices via `PriceFetcher` trait and cache in `daily_asset_prices`
4. NAV engine computes daily snapshots using unitization
5. `display.rs` renders tables, charts, and reports to the terminal

### Key design decisions

- **Prices as cents** — Transaction prices stored as `i64` cents in the DB, converted to `f64` for display
- **Daily prices as floats** — `daily_asset_prices` stores `f64` directly (API values)
- **NAV unitization** — First deposit sets NAV = 100.0; subsequent deposits issue shares at previous day's EOD NAV
- **Incremental rebuild** — Portfolio history rebuilds only from the last known snapshot
- **Effective end date** — Snapshots are never built for today; the end date is further limited by the slowest data source (handles fund NAV reporting delays)
- **Centralized constants** — All magic numbers live in `src/constants.rs`

## Database

SQLite database at `~/.rstock/rstock.db`, created automatically on first run. Schema managed via SeaORM migrations that run on startup.

### Migration commands

```bash
cd migration
cargo run -- up              # Apply pending migrations
cargo run -- down            # Rollback last migration
cargo run -- generate NAME   # Create new migration file
```

## Development

```bash
cargo build                              # Build
cargo test                               # Run all tests
cargo test --test nav_tests              # Run a specific test file
cargo test test_single_buy_initial_nav   # Run a single test by name
cargo test -- --nocapture                # Show stdout/stderr output
```

Tests use in-memory SQLite with migrations applied. The `PriceFetcher` trait enables mock-based testing without network calls. Use non-existent tickers (e.g. `XFAKE1`) to prevent real price lookups from overwriting test data.

### Environment variables

| Variable             | Description                                      |
|----------------------|--------------------------------------------------|
| `RSTOCK_SCRIPTS_DIR` | Override Python scripts directory lookup          |

## Documentation

- [`CLAUDE.md`](CLAUDE.md) — AI-oriented project guide (build commands, key rules, project structure)
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — System architecture, database schema, NAV algorithm, data flows
- [`docs/CONVENTIONS.md`](docs/CONVENTIONS.md) — Code conventions, naming patterns, testing guidelines, how-to guides
- [`TODO.md`](TODO.md) — Roadmap and known issues

## License

[MIT](LICENSE) — Pablo Lobato
