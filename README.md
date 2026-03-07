# rstock

A CLI portfolio tracker with NAV unitization, price fetching, and ASCII charts.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Overview

rstock tracks your investment portfolio from the terminal. It records buy transactions, fetches live prices from Yahoo Finance and Morningstar, computes daily portfolio NAV using unitization (the same method used by mutual funds), and renders performance charts directly in your terminal.

## Features

- **Transaction recording** — Log stock, ETF, and fund purchases with full metadata (fees, currency, notes, ISIN)
- **NAV unitization** — Daily portfolio valuation using unit-based accounting (first deposit = NAV 100, subsequent deposits issue shares at previous day's NAV)
- **Multi-source price fetching** — Stocks via Yahoo Finance (`yfinance-rs`), funds/ETFs via Morningstar (`mstarpy`)
- **Price caching** — Historical prices stored in SQLite with forward-fill for weekends/holidays
- **Performance tracking** — YTD, 1Y, 3Y, 5Y, and all-time returns
- **ASCII NAV chart** — Terminal-rendered portfolio performance chart via `textplots`
- **Colored portfolio table** — Per-asset breakdown with gain/loss highlighting

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
| `--notes`    | No       |         | Optional notes                       |

### View portfolio

```bash
rstock get                # Portfolio table + NAV summary + 1Y chart
rstock get --period ytd   # YTD chart
rstock get --period 5y    # 5-year chart
```

Chart periods: `ytd`, `1y` (default), `3y`, `5y`, `all`.

## Architecture

```
src/
├── cli.rs              # Clap CLI definition
├── main.rs             # Entry point, command dispatch
├── display.rs          # Terminal output: portfolio table, NAV chart
├── models/
│   ├── asset.rs        # AssetType enum, AssetInfo, Asset, AssetPosition
│   ├── portfolio.rs    # PortfolioSnapshot, PortfolioSummary, PortfolioResult
│   └── transaction.rs  # BuyOrder, Transaction
├── services/
│   ├── transactions.rs # Record purchases, invalidate snapshots
│   ├── portfolio.rs    # Portfolio summary and per-asset positions
│   ├── nav.rs          # NAV unitization engine
│   ├── daily_prices.rs # Price caching with forward-fill
│   └── price.rs        # PriceFetcher trait + Real/Mock implementations
├── db/
│   ├── mod.rs          # SQLite connection + auto-migration
│   ├── entities/       # SeaORM generated entities
│   └── repos/          # Repository layer
scripts/
├── get_fund_price.py          # Latest fund/ETF NAV (mstarpy)
└── get_fund_price_history.py  # Historical fund/ETF NAV (mstarpy)
migration/src/                 # SeaORM migrations
```

### Data flow

1. `cli.rs` parses commands
2. `main.rs` dispatches to service functions
3. Services fetch prices via `PriceFetcher` trait and cache in `daily_asset_prices`
4. NAV engine computes daily snapshots using unitization
5. `display.rs` renders tables and charts to the terminal

### Key design decisions

- **Prices as cents** — Transaction prices stored as `i64` cents in the DB, converted to `f64` for display
- **Daily prices as floats** — `daily_asset_prices` stores `f64` directly (API values)
- **NAV unitization** — First deposit sets NAV = 100.0; subsequent deposits issue shares at previous day's EOD NAV
- **Incremental rebuild** — Portfolio history rebuilds only from the last known snapshot
- **Effective end date** — Snapshots are never built for today; the end date is further limited by the slowest data source (handles fund NAV reporting delays)

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

## License

[MIT](LICENSE) — Pablo Lobato
