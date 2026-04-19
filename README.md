# rstock

A CLI portfolio tracker with NAV unitization, analytics, and terminal reporting.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Overview

rstock tracks your investment portfolio from the terminal. It records buy/sell transactions, dividends, and stock splits, fetches prices from Yahoo Finance and Morningstar-backed scripts, computes daily portfolio NAV using unitization, and renders portfolio and fund analysis directly in your terminal.

## Features

- **Transaction recording** — Log buys, sells, dividends, and stock splits for stocks, ETFs, and funds
- **NAV unitization** — Daily portfolio valuation using unit-based accounting (first deposit = NAV 100, subsequent deposits issue shares at previous day's NAV)
- **Multi-source price fetching** — Stocks via Yahoo Finance (`yfinance-rs`), funds/ETFs via Morningstar-backed Python scripts
- **Price caching** — Historical prices stored in SQLite with forward-fill for weekends/holidays
- **Multi-currency support** — All portfolio values converted to EUR base currency with cached FX rates
- **Performance tracking** — Portfolio and fund metrics including total return, CAGR, volatility, max drawdown, beta, and Sharpe ratio
- **ASCII NAV chart** — Terminal-rendered portfolio performance chart via `textplots`
- **Colored portfolio table** — Per-asset breakdown with gain/loss highlighting and dividend tracking
- **Portfolio composition** — Asset class, style, management, sector, country, and market-cap breakdowns with top holdings
- **Correlation analysis** — Portfolio asset correlation matrix over configurable periods
- **Stock monitoring** — Watchlist with momentum indicators (RSI, SMA, MACD), fundamentals, and sector comparison
- **Fund deep-dive analysis** — `analyze fund` with performance, top holdings, equity-only allocation tables, and snapshot diffs
- **Export** — Dump transactions to CSV
- **Asset listing** — View all tracked assets at a glance

## Prerequisites

- **Rust** (edition 2021)
- **SQLite** (bundled via `sqlx-sqlite`)
- **Python 3.10+** and **[uv](https://github.com/astral-sh/uv)** — required for the Morningstar-backed fund/ETF scripts

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
rstock portfolio asset add \
  --ticker MSFT \
  --name "Microsoft" \
  --type stock \
  --asset-class equity

rstock buy \
  --ticker MSFT \
  --date 26-02-2026 \
  --quantity 1 \
  --price 390
```

Create the asset once, then record transactions against it. The root `buy` command is a shortcut for `rstock transaction buy ...`.

Root `buy` flags:

| Flag         | Required | Default | Description                          |
|--------------|----------|---------|--------------------------------------|
| `--ticker`   | Yes      |         | Ticker symbol                        |
| `--date`     | Yes      |         | Purchase date (`DD-MM-YYYY`)         |
| `--quantity` | Yes      |         | Number of shares/units (fractional OK)|
| `--price`    | Yes      |         | Price per unit (e.g. `150.25`)       |
| `--fees`     | No       | `0`     | Commission/fees                      |

Asset metadata for funds/ETFs belongs on `rstock portfolio asset add/edit`, including `--morningstar-code`.

### Record a sell transaction

```bash
rstock transaction sell --ticker MSFT --date 01-03-2026 --quantity 0.5 --price 400
```

### Record a dividend

```bash
rstock transaction dividend --ticker MSFT --date 15-03-2026 --amount 25.50 --fees 3.80
```

### Record a stock split

```bash
rstock transaction split --ticker MSFT --date 20-03-2026 --ratio 2    # 2:1 split
rstock transaction split --ticker MSFT --date 20-03-2026 --ratio 0.25  # 1:4 reverse split
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
rstock portfolio list                       # Show all portfolio assets
rstock portfolio asset add --ticker SAN --name "Banco Santander" --type stock --asset-class equity
rstock portfolio asset add --ticker FUND1 --name "Sample Fund" --type fund --asset-class equity --management active --morningstar-code F00000YN5R
rstock data export --output txns.csv        # Export transactions to CSV
rstock data import --input txns.csv         # Import transactions from CSV
```

### Analyze portfolio and funds

```bash
rstock analyze composition                  # Portfolio composition breakdown
rstock analyze correlation                  # 1Y asset correlation matrix (default)
rstock analyze correlation --period 30d    # 30-day correlations
rstock analyze fund --code F00000YN5R      # Deep-dive fund analysis by Morningstar code
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
├── cli/                # Clap CLI: root shortcuts + grouped commands
├── main.rs             # Entry point, command dispatch
├── constants.rs        # Centralized constants (dates, currency, metrics, thresholds)
├── utils.rs            # Utility functions (scripts directory resolution)
├── lib.rs              # Public module exports
├── models/
│   ├── asset.rs        # AssetType enum, AssetInfo, Asset, AssetPosition
│   ├── portfolio.rs    # PortfolioSnapshot, CorrelationMatrix, composition and holdings models
│   ├── fund_analysis.rs# Fund analysis result and metrics models
│   ├── transaction.rs  # BuyOrder, SellOrder, DividendOrder, SplitOrder, TxType, Transaction
│   └── monitor.rs      # StockInfo, MomentumIndicators, RelationshipMetrics, MonitorReport
├── services/
│   ├── transactions.rs # Buy/sell/dividend/split recording + snapshot invalidation
│   ├── portfolio.rs    # Portfolio summary and per-asset positions
│   ├── nav.rs          # NAV unitization engine
│   ├── analytics.rs    # Portfolio analytics and correlation data
│   ├── composition.rs  # Portfolio composition analysis
│   ├── daily_prices.rs # Price caching with forward-fill
│   ├── exchange_rates.rs # FX rate caching (EUR base)
│   ├── price.rs        # PriceFetcher trait + Real/Mock implementations
│   ├── metrics.rs      # Beta, Sharpe, volatility, drawdown, CAGR, correlations
│   ├── fund_analysis.rs# Deep-dive fund analysis and snapshot diffing
│   ├── holdings.rs     # Fund/ETF holdings fetch helpers
│   ├── monitor.rs      # Stock analysis with momentum indicators
│   ├── import.rs       # Transaction CSV import
│   └── export.rs       # Transaction CSV export
├── db/
│   ├── mod.rs          # SQLite connection + auto-migration
│   ├── entities/       # SeaORM generated entities
│   └── repos/          # Repository layer (one per entity)
scripts/
├── get_fund_price.py          # Latest fund/ETF NAV
├── get_fund_price_history.py  # Historical fund/ETF price / total-return series
├── get_fund_data.py           # Fund metadata + holdings
└── get_fund_holdings.py       # Fund/ETF underlying holdings
migration/src/                 # SeaORM migrations
tests/                         # Integration tests + common test utilities
```

### Data flow

1. `src/cli` parses commands via clap derive macros
2. `main.rs` dispatches to service functions
3. Services fetch prices via `PriceFetcher` trait and cache in `daily_asset_prices`
4. NAV engine computes daily snapshots using unitization
5. `src/cli/display` renders tables, charts, and reports to the terminal

### Key design decisions

- **Prices as cents** — Transaction prices stored as scaled integers, converted to `f64` for display
- **Daily prices as floats** — `daily_asset_prices` stores `f64` directly (API values)
- **NAV unitization** — First deposit sets NAV = 100.0; subsequent deposits issue shares at previous day's EOD NAV
- **Incremental rebuild** — Portfolio history rebuilds only from the last known snapshot
- **Effective end date** — Snapshots are never built for today; the end date is further limited by the slowest data source (handles fund NAV reporting delays)
- **Shared CAGR calculation** — CAGR is centralized in `src/services/metrics.rs` and uses actual elapsed dates
- **Fund snapshot versioning** — Fund holdings snapshots are keyed by Morningstar's reported portfolio date, not by the local command run date
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
