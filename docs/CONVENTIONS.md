# Code Conventions

## Rust Conventions

- **Edition**: 2021, stable toolchain, no nightly features
- **Error handling**: `anyhow::Result<T>` for all fallible functions. Use `?` for propagation, `.context("message")` to add context, `anyhow::bail!()` for validation failures
- **Async**: All service and repository functions are `async`. Tokio runtime with full features. Use `async_trait` for async trait methods
- **No unwrap in production code**: Use `.context()` or pattern matching for fallible operations. `unwrap()` is acceptable only in tests and for known-safe literal conversions (e.g., `NaiveDate::from_ymd_opt` with hardcoded values)
- **Logging**: Uses `tracing` with `tracing-subscriber`. Initialized in `src/logging.rs`. Default level: `WARN`. CLI flag `-v`/`-vv`/`-vvv` increases verbosity; `RUST_LOG` env var is used as fallback when no `-v` flag is given. Logs are written to both stderr (colored, compact) and `~/.rstock/rstock.log` (daily rotation, no color). Use structured fields in tracing macros (e.g., `tracing::warn!(ticker, error = %e, "message")`) rather than string interpolation. Do NOT convert user-facing `println!` output to logging

## Function Ordering

Within each file, order functions top-to-bottom like a book: if function A calls function B, A appears above B. Public/entry-point functions go at the top, private helpers sink to the bottom.

## Module Organization

- `mod.rs` files re-export public types for ergonomic imports (see `src/models/mod.rs`)
- `lib.rs` declares modules (`pub mod db, models, services`) but does not re-export individual types. Constants and types live in one file; update all import paths directly
- Services contain business logic and call repos for data access. Services do not call other services' internal/private functions
- Repos are pure data access with no business logic. They accept `&impl ConnectionTrait` as the first parameter when an operation must support either `DatabaseConnection` or a caller-owned transaction

## Naming Conventions

### Functions

**Repository functions** follow a consistent naming pattern:
- `find_by_*()` — Query by a specific field (e.g., `find_by_ticker`, `find_by_asset_id`)
- `find_*_between()` — Range queries by date
- `find_at_or_before()` — Find the latest record on or before a date
- `find_latest()` / `find_earliest()` — Boundary queries
- `upsert()` — Insert or update (idempotent)
- `insert_*()` — Insert only (e.g., `insert_buy`, `insert_sell`)
- `delete_*()` — Cascade deletion (e.g., `delete_from_date`)
- `exists()` — Check existence

**Service functions** use verb-first naming:
- `get_portfolio()`, `compute_fund_analysis()`
- `buy()`, `sell()`, `dividend()`, `split()`
- `ensure_portfolio_history()`
- `fill_prices_for_range()`, `get_closing_price()`
- `compute_breakdown()`, `compute_correlation_data()`
- `generate_monitor_report()`
- `export_transactions_csv()`

### Types

Three categories of model structs:

| Category | Examples | Purpose |
|----------|----------|---------|
| Input structs | `AssetInfo`, `BuyOrder`, `SellOrder`, `DividendOrder`, `SplitOrder` | Data from CLI/caller, pre-persistence |
| DB-backed structs | `Asset`, `Transaction` | Domain objects with id, converted from entity::Model |
| Display structs | `CurrentPosition`, `CurrentPositions`, `PortfolioResult`, `CorrelationMatrix`, `CompositionResult`, `FundAnalysisResult`, `MonitorReport` | Computed values ready for rendering |

### General

- Functions and variables: `snake_case`
- Types and enums: `PascalCase`
- Constants: `UPPER_SNAKE_CASE`
- Modules: `lowercase`
- Enum variants: `PascalCase` (e.g., `Stock`, `Fund`, `Etf`)

## Database Patterns

- **ORM**: SeaORM with derive macros for entities. Entities are in `src/db/entities/`, auto-generated
- **Upsert**: `upsert()` uses SeaORM `on_conflict` directly; repositories must not perform a manual existence check before writing. `upsert_many()` is the bulk form for behaviorally equivalent writes and may execute bounded chunks inside the caller's connection or transaction. `insert_many_immutable()` is reserved for Historical market data, where successful completed-date observations are preserved and only failure markers may be replaced
- **Date storage**: Strings in `YYYY-MM-DD` format internally (DB, services), `DD-MM-YYYY` for user-facing input/output. See `display_date()` and `parse_date()` in constants/cli.
- **Monetary amounts**: Transactions use `i64` cents (`price_cents`, `fees_cents`). Daily prices use `f64` directly. Use `f64_to_cents()` before insertion and `cents_to_f64()` after retrieval
- **Migrations**: SeaORM migration crate in `migration/`. Migrations run automatically on database connect. Files are in `migration/src/` with timestamp-based naming
- **Connection**: Single SQLite connection created in `src/db/mod.rs`. Path: `~/.rstock/rstock.db`

## Testing Patterns

### Test Infrastructure

All test utilities are in `tests/common/mod.rs`:

- `setup_test_db()` — Creates in-memory SQLite with all migrations applied
- `MockMarketDataSources` — Implements `MarketDataSources` with configurable price/rate maps
- Helper functions: `insert_asset()`, `insert_transaction()`, `insert_sell_transaction()`, `insert_daily_price()`, `insert_exchange_rate()`, `get_portfolio_snapshot()`, `get_all_snapshots()`, `get_asset_snapshots()`

### Test Guidelines

- **Fake tickers**: Always use non-existent tickers (e.g., `XFAKE1`, `XFAKE2`) to prevent real API lookups from interfering with test data
- **In-memory DB**: Each test gets its own in-memory SQLite database. No cleanup needed; the connection is dropped after each test
- **Imports**: Tests import from `rstock::` (the library crate), not `crate::`
- **Assertions**: For floating-point comparisons, use `(value * 100.0).round() / 100.0` for 2-decimal precision or `assert!((a - b).abs() < epsilon)` for tolerance-based checks
- **No fixtures or property testing**: Tests build their own state imperatively using the helper functions
- **Test location**: Integration tests in `tests/` directory. Unit tests inline with `#[cfg(test)]` modules (e.g., `src/models/transaction.rs`)
- **Fixed clock**: Date-sensitive portfolio tests inject `FixedClock` through `MarketData` (normally via `market_data_at()`) and assert behavior through `get_current_positions()` or `get_portfolio()`. Tests specifically covering NAV-readiness ownership may call `ensure_portfolio_history()`. Do not call private rebuild helpers or rely on the machine date for these contracts
- **Nullable facts**: Test unavailable position facts independently and assert that aggregates are complete across their scope or `None`; do not accept partial sums as totals

### Test Files

| File | Coverage |
|------|----------|
| `tests/nav_tests.rs` | NAV unitization: empty portfolio, single/multiple buys, deposits at different NAVs, fee handling, sell transactions |
| `tests/integration_test.rs` | End-to-end scenarios combining buys, price changes, and portfolio queries |
| `tests/daily_price_tests.rs` | Price caching and forward-fill logic |
| `tests/portfolio_summary_tests.rs` | Portfolio computation and return calculations |
| `tests/current_positions_tests.rs` | Focused Transaction ledger inventory, shared performance/Monetary projection, nullable facts, and complete aggregates |
| `tests/current_date_tests.rs` | Fixed-clock portfolio and NAV-readiness behavior |
| `tests/individual_price_tests.rs` | Fixed-clock Live quote, Historical fallback, ETF capability, and mutual fund closing-price semantics |
| `tests/dividend_tests.rs` | Dividend recording and NAV cash accumulation |
| `tests/correlation_tests.rs` | Portfolio asset correlation matrix computation |
| `tests/monitor_tests.rs` | Momentum indicators and monitor report generation |

## How To: Add a New CLI Command

1. **Define the command** — Add a new variant to the appropriate clap enum in `src/cli/mod.rs` with attributes for all flags
2. **Add dispatch** — Add a match arm in `src/main.rs` that calls the appropriate service function
3. **Implement service logic** — Create or update a function in `src/services/`. Follow the verb-first naming pattern
4. **Add repo functions** — If new database operations are needed, add them to the relevant repo in `src/db/repos/`
5. **Integrate both output formats** — Pass `OutputFormat` into the CLI command adapter. Preserve the human renderer under `OutputFormat::Human`, and emit exactly one compact dotted-command envelope through `cli::output::emit_json()` under `OutputFormat::Json`
6. **Keep services presentation-neutral** — Return results or small mutation receipts from services. Do not print successful command output, terminal previews, or prompts from service code
7. **Write tests** — Add integration tests in `tests/` using the common test utilities. Cover representative human output and assert JSON structurally through `serde_json::Value`; do not snapshot field order. Add the new leaf path to the command-surface audit

Global `--json` is for successful application output only. New commands must not change the existing text behavior of runtime errors or Clap help/version. JSON schemas are unversioned and carry no backward-compatibility guarantee, so prefer direct serialization of display-ready results and introduce focused output structs only when user-facing units or identifiers require them.

## How To: Add a New Database Table

1. **Generate migration** — Run `cd migration && cargo run -- generate <name>` to create a new migration file
2. **Implement migration** — Write the `up()` and `down()` methods in the generated file using SeaORM's schema builder
3. **Register migration** — Add the new module to `migration/src/lib.rs` in the `Migrator` impl
4. **Create entity** — Add a new file in `src/db/entities/` defining `Model`, `ActiveModel`, `Column`, `Relation`, and implement `ActiveModelBehavior`. Register it in `src/db/entities/mod.rs`
5. **Create repository** — Add a new file in `src/db/repos/` with async query functions. Register it in `src/db/repos/mod.rs`
6. **Update models** — If needed, add domain structs in `src/models/` with `From<entity::Model>` conversions
