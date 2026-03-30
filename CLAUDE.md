# rstock

Rust CLI portfolio tracker with NAV unitization, multi-currency support, and ASCII charts. ~3,900 lines, edition 2021.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for system design and [docs/CONVENTIONS.md](docs/CONVENTIONS.md) for code patterns.

## Build, Test, Run

```bash
cargo build                              # Debug build
cargo build --release                    # Release build (binary at target/release/rstock)
cargo test                               # All tests
cargo test --test nav_tests              # Single test file
cargo test test_single_buy_initial_nav   # Single test by name
cargo test -- --nocapture                # Show stdout/stderr
cargo clippy                             # Lint
cargo fmt                                # Format
```

Run commands:

```bash
cargo run -- get                         # Portfolio summary + 1Y NAV chart
cargo run -- get --period ytd            # YTD chart (also: 1m, 3m, 6m, 3y, 5y, all)
cargo run -- buy --ticker MSFT --name "Microsoft" --type stock --date 2026-02-26 --quantity 1 --price 390
cargo run -- sell --ticker MSFT --date 2026-03-01 --quantity 0.5 --price 400
cargo run -- dividend --ticker MSFT --date 2026-03-15 --amount 25.50
cargo run -- split --ticker MSFT --date 2026-03-20 --ratio 2
cargo run -- list                        # Show all assets
cargo run -- export --output txns.csv    # Export transactions to CSV
cargo run -- holdings                    # Fund/ETF look-through
cargo run -- analyze portfolio           # Correlation matrix (default 1y)
cargo run -- monitor add --ticker AAPL --sector-etf XLK
cargo run -- monitor view AAPL           # Momentum + sector analysis
cargo run -- monitor list                # Show watchlist
cargo run -- monitor remove --ticker AAPL
```

Migrations:

```bash
cd migration && cargo run -- up          # Apply pending
cd migration && cargo run -- down        # Rollback last
cd migration && cargo run -- generate NAME  # New migration
```

## Verification

After any code change, run this checklist:

```bash
cargo fmt --check                        # Formatting
cargo clippy -- -D warnings              # Lint (treat warnings as errors)
cargo test                               # All tests (NAV, prices, portfolio, integration)
cargo build                              # Confirm it compiles
```

### Verifying financial metrics

The `get` command outputs NAV, returns, and risk metrics. After changing code in `services/nav.rs`, `services/portfolio.rs`, or `services/metrics.rs`, verify the output makes financial sense:

```bash
cargo run -- get                         # Check full output
cargo run -- get --period all            # Broader view
```

**What to check in the `get` output:**

- **NAV**: Should be a positive number. If the portfolio has only gained value since inception, NAV > 100.0 (initial NAV). If it has lost value, NAV < 100.0.
- **Daily change**: The absolute and percentage change between the last two snapshots. Sign should match (both positive or both negative).
- **YTD return**: Simple return `((current_nav - jan1_nav) / jan1_nav) * 100`. Should be consistent with the NAV chart trend since January 1.
- **1Y return**: Simple return from 365 days ago. Should reflect the chart's 1Y trend.
- **3Y/5Y CAGR**: Annualized using `((end/start)^(1/years) - 1) * 100`. Should be smaller in magnitude than the simple return over the same period. A 100% total return over 3 years is ~26% CAGR, not 33%.
- **Beta**: Measures correlation with ACWI benchmark. Typically 0.5–1.5 for a diversified stock portfolio. Values far outside this range suggest a data issue.
- **Sharpe ratio**: Excess return per unit of risk. Typically -1 to 3 for a real portfolio. Uses 3% annual risk-free rate and 252 trading days/year.
- **Portfolio table**: Gain/loss per asset should be green (positive) or red (negative). Weight column should sum to ~100%. Avg cost and current price should be in the asset's native currency.

**Automated test coverage for metrics:**

| Metric | Test file | What's tested |
|--------|-----------|---------------|
| NAV unitization | `tests/nav_tests.rs` | Single/multiple buys, sells, deposits at different NAVs, fees, multi-asset |
| YTD/period returns | `tests/portfolio_summary_tests.rs` | Simple return, CAGR formula, negative returns, period boundary lookup |
| Price caching | `tests/daily_price_tests.rs` | Forward-fill, cache hits, date gaps |
| End-to-end flows | `tests/integration_test.rs` | Buy + price change + portfolio query |
| Dividends | `tests/dividend_tests.rs` | Dividend recording and NAV cash accumulation |
| Correlations | `tests/correlation_tests.rs` | Portfolio asset correlation matrix computation |
| Monitor | `tests/monitor_tests.rs` | Momentum indicators and monitor report generation |

If you change a return formula or NAV calculation, add a test case in the corresponding file with known inputs and expected outputs before modifying the implementation.

## Key Rules

- **Monetary precision**: Transaction prices are stored as `i64` cents via `f64_to_cents`/`cents_to_f64` in `src/models/transaction.rs`. Daily asset prices and NAV values use `f64` directly. Never mix these representations.
- **Base currency**: EUR, hardcoded as `BASE_CURRENCY` in `src/constants.rs`. All portfolio values are converted to EUR for aggregation.
- **PriceFetcher trait**: `src/services/price.rs` defines the abstraction for all external price data. `RealPriceFetcher` hits Yahoo Finance (stocks) or Python/mstarpy scripts (funds/ETFs). Tests use `MockPriceFetcher` from `tests/common/mod.rs`. Never make network calls in tests.
- **Test tickers**: Use non-existent tickers (e.g., `XFAKE1`) in tests to prevent real price lookups from overwriting test data.
- **Test database**: Tests use in-memory SQLite via `setup_test_db()` in `tests/common/mod.rs`.
- **Snapshot invalidation**: When inserting a buy or sell transaction, delete `portfolio_history` and `portfolio_asset_history` rows from that date forward. This triggers a rebuild on the next `get` command. See `src/services/transactions.rs`.
- **Forward-fill**: Prices are forward-filled for weekends/holidays but never beyond the last date returned by the API. The effective end date in `src/services/nav.rs` is the minimum across all assets and exchange rates.
- **NAV unitization**: First deposit sets NAV = 100.0 and issues shares. Subsequent deposits issue shares at the current NAV. Sells redeem shares. See `process_day_transactions` in `src/services/nav.rs`.
- **No re-exports**: Constants and types live in one file; update all import paths directly rather than re-exporting.
- **Fund/ETF prices**: Come from Python scripts in `scripts/` run via `uv run`. The `RSTOCK_SCRIPTS_DIR` env var overrides script directory lookup.
- **Database**: SQLite at `~/.rstock/rstock.db`, auto-created on first run. Migrations run automatically on connect in `src/db/mod.rs`.
- **Dates**: Stored as `String` in `YYYY-MM-DD` format throughout domain models. CLI parses with `chrono::NaiveDate`.

## Environment Variables

| Variable | Description |
|---|---|
| `RSTOCK_SCRIPTS_DIR` | Override Python scripts directory lookup |

## Project Structure

```
src/
  main.rs              — Entry point, CLI command dispatch
  cli.rs               — Clap CLI definitions (get, buy, sell, dividend, split, list, export, holdings, analyze, monitor)
  constants.rs         — Centralized constants (dates, currency, metrics, thresholds)
  display.rs           — Terminal tables (tabled), NAV chart (textplots), reports
  utils.rs             — Utility functions (scripts directory resolution)
  lib.rs               — Public module exports
  models/
    asset.rs           — AssetType enum, Asset, AssetInfo, AssetPosition
    portfolio.rs       — PortfolioSnapshot, PortfolioSummary, CorrelationMatrix, holdings models
    transaction.rs     — BuyOrder, SellOrder, DividendOrder, SplitOrder, TxType, Transaction, cents helpers
    monitor.rs         — StockInfo, MomentumIndicators, RelationshipMetrics, MonitorReport
  services/
    nav.rs             — NAV unitization engine (rebuild_portfolio_history)
    portfolio.rs       — Portfolio summary, return calculations
    transactions.rs    — Buy/sell/dividend/split recording + snapshot invalidation
    daily_prices.rs    — Price caching with forward-fill
    exchange_rates.rs  — FX rate caching (EUR base)
    price.rs           — PriceFetcher trait + RealPriceFetcher
    metrics.rs         — Beta, Sharpe, volatility, drawdown, correlation matrix (ACWI benchmark)
    holdings.rs        — Fund/ETF look-through holdings report
    monitor.rs         — Stock analysis with momentum indicators and sector comparison
    export.rs          — Transaction CSV export
  db/
    mod.rs             — SQLite connection + auto-migration
    entities/          — SeaORM generated models (8 tables, including watchlist)
    repos/             — Repository layer (one per entity, including watchlist_repo)
migration/src/         — SeaORM schema migrations (5 migrations)
tests/                 — Integration tests + common test utilities
scripts/               — Python helpers for fund/ETF prices and holdings (mstarpy)
```
