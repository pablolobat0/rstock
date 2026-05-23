# rstock (Rust CLI Portfolio Tracker)
**References:** Read `docs/ARCHITECTURE.md` (design) and `docs/CONVENTIONS.md` (patterns) before modifying code.

## Execution & Quality
* **Verification:** `cargo fmt && cargo clippy -- -D warnings`
* **Tests:** `cargo test`. Never make network calls. Use `setup_test_db()` (in-memory SQLite) and dummy tickers (e.g., `XFAKE1`).
* **Migrations:** `cd migration && cargo run -- [up|down|generate NAME]`
* **Logging:** `tracing` macros for diagnostic output. Never convert `println!` (user-facing) to logging. Use structured fields: `tracing::warn!(ticker, error = %e, "message")`.

## Core Domain Rules
* **Precision:** Transaction prices use `i64` cents (`src/models/transaction.rs`). Daily asset prices and NAV use `f64`.
* **Currency:** EUR base (`BASE_CURRENCY` in `src/constants.rs`). All values aggregate to EUR.
* **Tickers:** Universal identifier (`-t`). Stocks = Symbol (MSFT). Funds/ETFs = ISIN (IE00B4L5Y983). Hide ticker column for funds in UI.
* **Dates:** Internal/DB = `YYYY-MM-DD`. Display/CLI Input = `DD-MM-YYYY` (custom parser in `src/cli.rs`).

## System Behaviors
* **Market Data:** Use the stateful `MarketData` module (`src/services/market_data/`). Yahoo and Morningstar source adapters are private implementation details behind `MarketDataSources`.
* **Data Filling:** Forward-fill prices for weekends/holidays up to the minimum effective end date across all assets/FX.
* **NAV Unitization:** Initial deposit sets NAV=100.0. Subsequent deposits issue shares at current NAV; sells redeem shares (`src/services/nav.rs`).
* **Snapshots Invalidation:** Buy/sell transactions delete `portfolio_history` and `portfolio_asset_history` from the transaction date forward to trigger rebuilds.
* **Database:** SQLite at `~/.rstock/rstock.db`. Auto-migrates on connection (`src/db/mod.rs`).

## Agent skills

### Issue tracker

Issues and PRDs are tracked as local markdown files under `.scratch/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the default five-role triage vocabulary. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context repo: read root `CONTEXT.md` and `docs/adr/` when present. See `docs/agents/domain.md`.
