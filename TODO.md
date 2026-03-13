# rstock — TODO

## Bugs / Code Quality

- [ ] Fix silent asset_type defaulting — `src/models/asset.rs:62` uses `unwrap_or(AssetType::Stock)` for invalid types; should log a warning or return an error
- [x] Add date validation on `buy` — currently accepts any string; should parse with chrono at CLI level
- [ ] Prevent duplicate transactions — nothing stops recording the exact same buy twice
- [x] Extract cents<->float helper — `price_cents as f64 / 100.0` is scattered across services

## New Commands

- [ ] `list` — show all assets and/or transactions (with filters: by ticker, date range, type)
- [ ] `delete` — remove a transaction by ID (with confirmation prompt)
- [ ] `sell` — record a sell transaction (tx_type="sell", reduce quantity, track realized gains)
- [ ] `get --ticker MSFT` — show single-asset detail view with individual price chart
- [ ] `update` — fetch and cache latest prices without displaying portfolio
- [ ] `export` — dump portfolio/transactions to CSV or JSON
- [ ] `import` — bulk import transactions from broker CSV exports

## New Transaction Types

- [ ] Sell transactions — schema already has `tx_type` field (hardcoded to "buy"); add CLI command + service logic
- [ ] Dividends — new tx_type, option for reinvest vs. cash
- [ ] Stock splits — adjust quantity and cost basis for affected asset

## Portfolio Features

- [ ] Realized vs. unrealized gains — requires FIFO/LIFO lot tracking
- [ ] Tax lot tracking — cost basis per lot for tax reporting
- [ ] Benchmark comparison — compare portfolio NAV against S&P 500, MSCI World, etc.
- [ ] Risk metrics — volatility, Sharpe ratio, max drawdown
- [ ] Sector/country allocation breakdown
- [ ] Rebalancing alerts — target allocation vs. current weights
- [x] Multi-currency support + forex conversion (currently all treated as same currency)

## Display Improvements

- [x] Color portfolio table rows — green for gaining assets, red for losing
- [ ] `--dry-run` flag for buy — show what would be recorded without committing
- [ ] Interactive TUI mode (ratatui) — navigate assets, drill into history
- [ ] Sparkline per asset in the portfolio table

## Structural Improvements

- [ ] Move chart date-range logic out of `main.rs` into a service or display helper
- [ ] Use `chrono::NaiveDate` in domain models instead of String (convert at DB boundary)
- [ ] Add structured logging (`tracing` crate) — replace `eprintln!()` warnings
- [ ] Config file (`~/.rstock/config.toml`) — default currency, DB path, chart period, etc.
- [ ] Database backup/restore command

## Test Gaps

- [ ] CLI parsing tests (invalid args, missing required flags)
- [ ] Display output tests (snapshot testing for table/chart output)
- [ ] Error path tests (invalid dates, DB failures, network timeouts)
- [ ] Duplicate transaction handling
- [ ] Full binary integration tests (`cargo run` end-to-end)
- [ ] Performance tests with large portfolios (100+ assets, years of history)
