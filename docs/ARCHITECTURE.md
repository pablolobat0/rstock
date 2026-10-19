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
- **`benchmark`** — Benchmark definitions, explicit primary assignments
- **`blend`** — Create, inspect and delete saved Custom benchmark blends
- **`strategy`** — Explicitly select, unselect and inspect the portfolio's Strategy benchmark

`main.rs` is a thin process bootstrap: it parses the CLI with clap, initializes logging, connects to the database, and builds the `MarketData` sources, then delegates the parsed command to `cli::run_command()` in `src/cli/dispatch.rs`. That library-owned dispatch routes every command in the current surface — both dashboard aliases `get` and `portfolio get` reach the same path — so production, integration tests, and benchmarks exercise one application module graph. The Portfolio dashboard commands compose `portfolio_view::get_portfolio_view()` first, then write the complete outcome through the separate human and JSON output Adapters in `src/cli/adapters/`, which accept injected `Write` targets and own sorting, labels, the nested JSON schema, and the NAV chart (ADR-0004). Other command adapters in `src/cli/commands/` call presentation-neutral services, then choose either the existing human renderer or `output::emit_json()`. `src/cli/output.rs` owns compact serialization and emits one `command`/`data` envelope to stdout; services do not emit successful command output. Errors and Clap help/version remain outside this successful-output boundary.

The library owns the complete command dispatch, and the executable imports the shared modules from the `rstock` library rather than compiling a second copy of them. Logging initialization remains owned by the executable. Test bodies and helpers live in dedicated files under `tests/`; private unit modules reference `tests/unit/` files through test-only module declarations.

### Logging (`logging.rs`)

Structured logging via `tracing`. Two output layers:
- **stderr** — Colored, compact format. Controlled by `-v` flag or `RUST_LOG` env var. Default: warnings only.
- **File** — Daily-rotating log at `~/.rstock/rstock.log`. Full timestamps, includes module target path.

Verbosity mapping: default=WARN, `-v`=INFO, `-vv`=DEBUG, `-vvv`=TRACE.

### Service Layer (`services/`)

All business logic lives here. Key modules:

**`ledger.rs`** — Pure canonical transaction replay and native-currency effects, with separate in-memory FX enrichment. Owns `(date, id)` order and every-prefix validity; mutation and valuation services consume its transitions. See [ledger interface, operational boundaries, and PR77 closure](r77-ledger-closure.md) for the typed-data placement decision, precision, zero-NAV policy, and verification evidence.

**`nav.rs`** — Core NAV unitization engine. The public `ensure_portfolio_history()` interface owns readiness for the latest completed date and returns the latest snapshot together with NAV-scoped Market data limitations; it prepares one private immutable `NavRebuildPlan` from the trusted latest Complete NAV snapshot, ordered ledger effects, Positive-holding intervals, and MarketData valuation series. Plan execution performs no database or MarketData reads, advances only through the contiguous Effective valuation date prefix, processes share issuance and redemption, and calculates end-of-day portfolio value through strict in-memory valuation reads.

**`market_data/historical.rs`** — Private implementation for reproducible Historical market data used by NAV and benchmark analytics. It reads persisted coverage, requests only missing contiguous asset and FX intervals, resolves strict-before predecessor observations through the source Interface when needed, bulk-persists successful observations without replacing covered dates, prepares NAV valuation series for NAV-owned Positive-holding intervals and transaction FX requirements, hides provider-specific FX pair construction from external callers, and exposes strict valuation reads through the `market_data` Module root. NAV prepares independent asset series and FX currencies concurrently under the shared four-source-request limit; disjoint intervals for one asset remain sequential to preserve cache/predecessor ordering. A `MarketData` instance shares identical source attempts and their results for the lifetime of one command; a new command can retry failures.

**`market_data/reference.rs`** — Private cache-first preparation of persisted benchmark references. Stable reference definitions and independent reference-price observations do not create asset or Transaction rows; source lookup identity, currency and proxy qualifications come from the reference definition. Completed observations are immutable, missing intervals are requested through the existing `MarketData` source coordinator, and reference prices are converted to EUR through the shared FX cache.

**`market_data/individual_price.rs`** — Private implementation for display-time Individual price values for portfolio rows. Stocks request same-day observations, and ETFs request them through the fund-price capability already exposed by `MarketDataSources`; neither caller depends on a concrete source Adapter. A same-day observation is a Live quote. If an ETF source cannot supply one, the row falls back to the latest Historical market data. Mutual funds retain closing-price semantics and never use same-day Live quotes.

**`portfolio_inventory.rs`** — `get_portfolio_inventory()` is the focused Portfolio inventory Interface and does not request NAV readiness, so it never creates or rebuilds NAV history. Historical preparation uses each asset's own first transaction date rather than another holding's earlier history. `MarketData` reuses NAV price preparation only for assets whose attempted holding interval reaches the requested end date; historical ledger FX remains independently prepared and consolidated by currency. Monetary holdings and new holdings outside NAV's prepared intervals still receive price preparation. One Transaction ledger projection derives quantity, remaining cost, dividends, and Open-position gain/loss identically for performance-holding and Monetary-holding sections, which separately own their positions, complete-or-unavailable aggregates, and Market data limitations; the combined informational Total value is available only when both section values are complete. Calculated financial facts carry explicit available, unavailable, or not-applicable `FactAvailability` states while descriptive metadata stays unwrapped. Composition analysis consumes this Interface directly.

**`portfolio_view.rs`** — `get_portfolio_view()` is the presentation-neutral Portfolio view composer. It owns NAV readiness, composes the focused inventory, expresses core Portfolio performance as not applicable/unavailable/available, and includes the requested ready NAV history before returning. Available performance carries cohesive YTD, 1Y, 3Y, 5Y, and All period outcomes over the ready NAV series with independently available return and risk facts, and exposes the benchmark-dependent risk limitations as their own scope.

**`benchmark.rs`** — Shared comparison calculation. `compute_standard_benchmark_periods()` measures the YTD, 1Y, 3Y, 5Y, and since-comparison-start rows over aligned subject/reference series, independent of the graph selection, and returns the shared neutral `BenchmarkPeriod` row with its independent availability. The fund comparison maps that row into its own accepted `FundBenchmarkPeriod` output row, and the exposure-matched portfolio comparison uses the shared row directly; both carry the same computed facts. Trailing anchors use the latest common close on or before the requested date so a weekend or market holiday never shortens the period, and every row exposes its actual anchor.

**`exposure_benchmark.rs`** — The exposure-matched primary benchmark of the Portfolio dashboard. `compute_exposure_matched_comparison()` combines the explicitly assigned primary reference of every held asset through the EUR exposures of the *immediately preceding* Complete NAV close, so an interval is weighted by the exposures that were actually held at the last close before it, including the effect of a trade or a dividend on a non-trading day. The complete NAV scope is the denominator of every weight, Monetary holdings are excluded, and the retained Net dividend income inside NAV is carried as a 0%-return residual. Assets sharing one primary reference aggregate their weights, secondary and candidate references never enter them, and a missing assignment or component makes the comparison unavailable instead of renormalizing the remaining weights. Both series are anchored at 100 on the first invested closing snapshot, the omitted seed-to-first-close interval is disclosed, and explicit assignments are reported as retrospective without splicing. The comparison reuses the already ready NAV history, reads the per-asset exposures with one ordered range query, prepares each contributing reference once through the shared `MarketData` path, and performs no ledger replay, no second NAV rebuild, no per-date query, and no per-date source request. Its own availability is independent: a benchmark failure never removes the inventory, the NAV readiness, or the portfolio-only outcomes. See [exposure-matched benchmark performance evidence](exposure-benchmark-performance-evidence.md) for the recorded work evidence.

**`analytics.rs`** — Computes asset-series correlation from current Transaction ledger holdings and historical Base currency series, and computes portfolio risk metrics from NAV history and benchmark prices. Asset-series correlation does not rebuild NAV history.

**`composition.rs`** — Builds portfolio composition analytics with look-through aggregation and top holdings.

**`custom_blends.rs`** — Saved Custom benchmark blends. A blend is a flat, long-only, fully allocated definition of explicit components with optional synthetic 0%-return cash; it is saved once and never edited or automatically revised. The admission window is a real comparison boundary: by default the portfolio's ready first invested Complete NAV snapshot through its effective complete comparison end, or an explicitly selected later start, with the requested and the actual completed weekday closes both persisted. A requested boundary that falls on a weekend resolves to the preceding completed weekday by calendar rule before any component availability is inspected, and every component must hold the required anchor and end closes plus every interior weekday, so a blend never opens or ends on a shortened window. The preview compounds arithmetic beginning-of-interval component returns, resets target weights at the close immediately preceding each new calendar month, lets the weights drift with component wealth between resets, and reports the component, reset, date, and convention provenance of the result. A blend of synthetic cash alone has no market observation, so it is calculated over the resolved completed weekdays of its window at a constant wealth, without any market request, fee, or transaction. Creation and deletion each run in one caller-owned transaction, so a failed write leaves no partial definition and a failed deletion leaves the saved definition, its components, and every price, FX, ledger, and NAV row intact.

**`strategy.rs`** — The explicitly selected Strategy benchmark. A Strategy benchmark is a saved Custom blend the owner chooses on purpose: it is never inferred from the holdings, and it carries no target allocation or weight. The selection is persisted in its own table with a foreign key on the selected definition and a single selection slot as its primary key, so the database itself holds at most one selected Strategy benchmark and a selected definition cannot be deleted behind the owner's back. The comparison reuses the already-ready NAV history and the selected blend's own monthly-reset wealth, and aligns both series at the first close they share, so it never measures unmatched endpoints; every completed weekday of the window must be covered by both series, so a missing close leaves the comparison unavailable instead of silently shortening it. The window ends at the last completed weekday on or before the Effective valuation date, like the exposure-matched comparison, so a Strategy benchmark is never asked for a close the portfolio does not have either. A Strategy benchmark that starts after the portfolio exposes its own anchor and the coverage it cannot offer, and that anchor and coverage stay visible even when a later required input is missing. The comparison is a reference comparison only: it never changes the exposure-matched primary comparison, the portfolio's weights, its NAV accounting, its inventory, or its Effective valuation date. Only a typed missing observation is an unavailable comparison; a database failure, corrupt persisted data, and a violated recorded convention remain errors, and the selected definition and its provenance stay visible in the unavailable outcome.
**`transactions.rs`** — `buy()`, `sell()`, `dividend()`, `split()`, `edit()`, and `delete()` tentatively apply each Transaction ledger change, replay every affected asset ledger from zero, and commit the mutation with all dependent invalidation in one database transaction. Ordinary mutations invalidate Complete NAV snapshots from the earliest affected transaction date (the old or new date for edits); split creation, editing, or deletion also clears the asset's entire split-adjusted price cache and invalidates snapshots from the asset's earliest transaction date. Ledger reads use explicit `(date, id)` chronology, while replay owns canonical ordering and prefix validity.

**`assets.rs`** — Tracked asset edits that cross the Monetary/performance boundary invalidate every dependent Complete NAV snapshot from the asset's earliest transaction date. Fund/ETF provider-identity changes also clear that asset's price cache and invalidate the same complete dates. Metadata updates, cache deletion when required, and portfolio/all-per-asset snapshot invalidation commit atomically so readiness can continue trusting the retained checkpoint.

**`market_data/`** — Stateful market data Module. It exposes use-case-shaped Interfaces for valuation market data, correlation market data, benchmark-reference market data, Individual price, stock info, and fund data. Yahoo Finance and Morningstar source Adapters are private implementation details behind `MarketDataSources`.

**`metrics.rs`** — Shared math helpers for volatility, max drawdown, Sharpe, Sortino, beta, Pearson correlation, log returns, return alignment, and CAGR. Uses daily log returns, 252 trading days/year, 3% annual risk-free rate, and actual elapsed dates for CAGR. A metric that is mathematically undefined for the given series (zero-variance benchmark beta, zero-dispersion Sharpe, no-downside Sortino) is `None`, so consumers can distinguish undefined metrics from computed values.

**`fund_analysis.rs`** — `compute_fund_analysis()` builds a deep-dive report for any Morningstar fund code, including performance, holdings, equity-only allocation tables, and holdings snapshot diffs. Snapshot persistence is keyed by Morningstar's reported portfolio date, so repeated runs against the same Morningstar snapshot do not create duplicate history rows.

**`holdings.rs`** — Shared holdings fetch helper used by composition and other look-through paths.

**`monitor.rs`** — `generate_monitor_report()` fetches price history for a stock and its sector ETF, computes momentum indicators (RSI-14, SMA-50/200, MACD), fundamentals, and relative strength metrics.

**`export.rs`** — `export_transactions_csv()` dumps all transactions to a CSV file.

### Output Layer (`src/cli/adapters/`, `src/cli/display/`, `src/cli/output.rs`)

Pure output formatting with no business logic. `OutputFormat` selects human or JSON output at the CLI boundary. The Portfolio dashboard commands use the dedicated output Adapters, which write one complete Portfolio view through an injected `Write` target; the other human renderers print through `display` submodules:

- **`adapters/human.rs`** — the human Portfolio view Adapter: per-section inventory tables and aggregates, synchronized performance facts with the Effective valuation date, the metrics table, the NAV chart for the requested history, the exposure-matched benchmark section with its aligned subject/reference graph, standard rows, and reference provenance, semantic not-applicable/unavailable fact labels, and the four limitation scopes
- **`adapters/json.rs`** — the JSON Portfolio view Adapter: one compact envelope over the nested inventory/performance schema, the exposure-matched benchmark with its availability, aligned series, standard rows, and reference provenance, `null` mapping for unavailable and not-applicable scalar facts, requested NAV history, and the four structured limitation scopes
- **`helpers.rs`** — Shared formatting utilities (price, quantity, percentage, color helpers, limitation wording)
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
- `InventoryPosition` — Current Transaction ledger holding with independently available cost, dividend, Individual price, value, and Open-position gain/loss facts

**`transaction.rs`**:
- `TxType` enum — Buy, Sell, Dividend, Split (with `Display`/`FromStr` for DB serialization)
- `BuyOrder`, `SellOrder`, `DividendOrder`, `SplitOrder` — Input structs for recording transactions
- `Transaction` — DB-backed semantic transaction struct whose type-specific fields map to typed ledger entries at the replay boundary
- `f64_to_cents()` / `cents_to_f64()` — Conversion helpers for monetary precision

**`portfolio.rs`**:
- `PortfolioSnapshot` — Daily NAV snapshot (date, asset_value, total_value, outstanding_shares, nav)
- `AssetSnapshot` — Per-asset daily position (quantity, closing_price, market_value, exchange_rate)
- `CorrelationMatrix` — N×N asset correlation matrix with labels and warnings
- `AllocationEntry`, `TopHolding`, `CompositionResult`, `FundHolding` — Composition and holdings models

**`portfolio_inventory.rs`**:
- `FactAvailability<T>` — Portfolio fact availability: Available value, Unavailable because a required input is missing, or NotApplicable for a domain reason
- `IndividualPrice` — One Individual price fact with its native price and observation date
- `InventoryPosition` — One current holding with unwrapped descriptive metadata, known quantity, availability-wrapped calculated financial facts, and position Market data limitations
- `InventorySectionAggregates` — Complete-or-unavailable section sums; empty sections are available zero
- `PortfolioInventorySection` — One section's positions, aggregates, and Market data limitations
- `PortfolioInventory` — Separate performance-holding and Monetary-holding sections with the combined informational Total value
- `NavHistoryRequest` — Presentation-neutral request for ready NAV history, consumed by the Portfolio view composer
- `PortfolioPerformance` / `AvailablePortfolioPerformance` — Core performance outcome state and its non-nullable synchronized facts, five cohesive period outcomes with independently available return and risk facts, and NAV/history plus benchmark-dependent risk limitation scopes
- `PeriodOutcome` / `PerformancePeriods` — One period's return, volatility, max drawdown, beta, Sharpe, and Sortino facts with independent availability, and the YTD/1Y/3Y/5Y/All group
- `PortfolioView` — Always-present Portfolio inventory, core performance state, and requested ready NAV history

**`benchmark.rs`**:
- `BenchmarkPeriod` — The shared computed comparison period: subject/reference return and risk facts with independent availability
- `BenchmarkComparisonPeriod` — The YTD, 1Y, 3Y, 5Y, and since-comparison-start periods
- `ComparisonAvailability`, `ComparisonFact<T>` — The explicit available/unavailable/not-applicable states of a comparison and of one calculated fact
- `DatedWealthPoint` — One dated point of an aligned wealth series

**`fund_analysis.rs`**:
- `FundBenchmarkComparison`, `FundBenchmarkPeriod` — The fund comparison outcome and its accepted `fund_return_pct` period row, mapped from the shared computed row without changing the fund's JSON contract
- `FundAnalysisResult` — Full fund report including top holdings, allocations, period metrics, and holdings snapshot diff
- `FundPeriodMetrics` — Total return, CAGR, volatility, max drawdown, beta, Sharpe, and Sortino
- `HoldingChange`, `HoldingChangeType`, `FundData` — Snapshot diff and Morningstar payload models

**`exposure_benchmark.rs`**:
- `ExposureBenchmarkComparison` — The exposure-matched primary benchmark: its availability, the aligned subject/reference wealth series, the standard periods, the contributing references, and the omitted seed-to-first-close interval
- `ExposureBenchmarkReference` — One contributing primary reference with its assets, aggregated weights, and full provenance
- `OmittedSeedInterval` — The seed-to-first-close interval the comparison omits

**`monitor.rs`**:
- `StockInfo` — Fundamentals (price, P/E, EPS, dividend yield, market cap, sector, etc.)
- `MomentumIndicators` — RSI-14, SMA-50/200, MACD with signal interpretations
- `RelationshipMetrics` — Relative strength, beta vs sector, correlation
- `MonitorReport` — Combined report with stock/sector momentum and price series

### DB Layer (`db/`)

**`mod.rs`** — `connect()` creates the SQLite connection at `~/.rstock/rstock.db`, auto-creates parent directories, and runs pending migrations via `sea_orm_migration::MigratorTrait::up()`.

**`entities/`** — SeaORM-generated model structs for each table, including `fund_holdings_snapshot`. Each entity defines `Model`, `ActiveModel`, `Column`, and `Relation` types.

**`repos/`** — Repository pattern with one module per entity (including `watchlist_repo`). Functions are `async` and accept a SeaORM connection interface as their first parameter so callers can use either the application connection or a caller-owned transaction. Standard function naming: `find_*`, `find_by_*`, `find_*_between`, `find_at_or_before`, native-conflict `upsert`, bulk `upsert_many`, `insert_*`, and `delete_*`. Repository writes do not check existence before writing; callers own transaction boundaries when a mutation spans repositories.

### Constants (`constants.rs`)

Centralized constants: `BASE_CURRENCY`, `INITIAL_NAV`, period durations, benchmark configuration (`ACWI`), risk-free rate, trading days, momentum indicator parameters (RSI, SMA, MACD), and floating-point thresholds.

### Utilities (`utils.rs`)

`confirm_action()` provides a shared interactive confirmation prompt for destructive user-facing operations.

## Database Schema

Current schema includes the original portfolio tables plus watchlist, fund holdings snapshots, and reference-only benchmark definitions and cached prices.

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

**`transactions`** — Semantic Transaction ledger records
| Column | Type | Notes |
|--------|------|-------|
| id | i32 PK | Auto-increment |
| asset_id | i32 FK | References assets.id |
| tx_type | String | "buy", "sell", "dividend", or "split" |
| date | String | YYYY-MM-DD |
| units | f64 nullable | Positive units for buys/sells only |
| unit_price_cents | i64 nullable | Positive native-currency unit price for buys/sells only |
| dividend_amount_cents | i64 nullable | Positive Gross dividend distribution for dividends only |
| dividend_deductions_cents | i64 nullable | Non-negative dividend deductions, bounded by gross |
| split_ratio | f64 nullable | Positive new-units-per-old-unit ratio for splits only |
| fees_cents | i64 nullable | Non-negative buy/sell fees only |
| created_at | String | ISO timestamp |

Chronological ledger access is indexed by `(date, id)`, while per-asset access is indexed by `(asset_id, date, id)`.

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
| cash_balance | f64 | Legacy physical column, default 0.0; excluded from the current ORM model |
| asset_value | f64 | Total market value in EUR |
| total_value | f64 | Total portfolio value in EUR |
| outstanding_shares | f64 | NAV shares outstanding |
| nav | f64 | NAV per share |

Accumulated net dividend income is represented by `total_value - asset_value`.
The original migration's `cash_balance` column remains in SQLite but is not
read or written by the current ORM model.

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

`benchmark_references` owns stable reference identity and source/return-basis provenance; `benchmark_reference_prices` stores its immutable completed-date observations independently of `assets` and holdings. The additive legacy-proxy migration copies compatible existing ACWI/USD stock-cache rows into the identified ETF-proxy cache without deleting or relabeling their original rows.

`benchmark_blends` holds the blend-specific facts of a saved Custom blend, `benchmark_blend_components` holds its component membership, target weights, and per-component convention snapshots, and `benchmark_references.kind` distinguishes a saved blend from a source-backed reference. A saved blend keeps one stable definition identity, so it is assigned, guarded, and deleted like any other definition; its components reference other definitions, never itself. Rolling the blend migration back is refused while saved blends exist, so a blend is never silently converted into an ordinary catalog reference.

`benchmark_strategy_selections` holds the optional selected Strategy benchmark. The table records only the chosen definition identity, when it was chosen, and why: the selection is always explicit, and no strategy or target allocation is ever inferred from the holdings. Its single selection slot is the primary key and a check constraint pins that slot to the one allowed value, so the database itself holds at most one selected Strategy benchmark even against a direct write, and its foreign key on `benchmark_references.id` restricts the deletion of a selected definition. Rolling the selection migration back is refused while a selection exists, so the owner's explicit choice is never silently lost.

## NAV Unitization Algorithm

The NAV engine (`src/services/nav.rs`) uses the same valuation method as mutual funds:

1. **Initialization**: A new portfolio starts with NAV = 100.0 and zero shares. When the first calculable performance transaction is processed, the engine also persists a zero-value seed snapshot for the previous day.

2. **Daily iteration**: For each calendar day from start to effective end:

   a. **Process transactions for this day**:
      - **Buy (deposit)**: If it's the first deposit (outstanding_shares == 0), set NAV = 100.0 and issue `deposit_amount / 100.0` shares. Otherwise, issue `deposit_amount / current_nav` new shares at the current NAV.
      - **Sell (withdrawal)**: Calculate proceeds, redeem `proceeds / current_nav` shares, reducing outstanding shares.

   b. **Compute end-of-day values**: For each held asset, look up closing price and exchange rate, calculate `quantity * price * fx_rate`. Sum all positions for total portfolio value.

   c. **Calculate NAV**: `nav = total_value / outstanding_shares`

   d. **Store snapshot**: Write the day's `portfolio_history` and `portfolio_asset_history` records, persisted in writes bounded by both date and row targets so a persisted snapshot is never split (see [ADR-0004](adr/0004-execute-nav-from-a-prepared-rebuild-plan.md)).

3. **Effective valuation date**: The rebuild never extends beyond yesterday and stops at the end of the contiguous prefix whose Positive-holding intervals and transaction-date FX requirements are present in the prepared series. This prevents extrapolation or skipping when data sources lag.

4. **Readiness and incremental rebuild**: Callers use `ensure_portfolio_history()`. It trusts the latest Complete NAV snapshot as the NAV rebuild checkpoint, then prepares and executes one complete private plan from the following day (or from the first transaction date for a new portfolio).

## NAV Return vs Open-Position G/L%

The `get` command displays two figures that answer different questions:

- **Open-position G/L%**: `(current_value - remaining_weighted_average_cost) / remaining_weighted_average_cost` for units still held. It excludes dividends and realized gains from sold units.
- **NAV return (time-weighted)**: Measures portfolio unit performance from inception (NAV 100.0). Unaffected by deposit timing because each deposit issues new shares at the current NAV.

These diverge whenever cash-flow timing, dividends, or sales matter. NAV measures performance independent of external cash flows, while Open-position G/L describes only the unrealized price result of inventory that remains open.

Example: after a partial sale, realized proceeds and gains no longer contribute to Open-position G/L, while historical NAV performance remains represented in the NAV series.

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
rstock::cli::run_command
  └─> commands::portfolio::get()
        ├─> Map the CLI chart period onto NavHistoryRequest
        ├─> portfolio_view::get_portfolio_view()
        │     ├─> nav::ensure_portfolio_history()
        │     │     ├─> Prepare one immutable NavRebuildPlan from the trusted checkpoint
        │     │     └─> Execute the plan with strict in-memory valuation reads
        │     ├─> portfolio_inventory::get_portfolio_inventory()
        │     │     ├─> Project all open holdings once from the Transaction ledger
        │     │     ├─> Resolve each position's Individual price through MarketData
        │     │     ├─> Separate performance and Monetary sections
        │     │     └─> Build complete-or-unavailable section aggregates
        │     ├─> Compose the YTD/1Y/3Y/5Y/All period outcomes over the ready NAV series
        │     └─> Slice the requested ready NAV history into the same outcome
        └─> Choose the output Adapter (both aliases share this path)
              ├─> adapters::write_portfolio_json() via the injected stdout writer
              └─> adapters::write_portfolio_human() via the injected stdout writer
```

### `transaction buy`

```
rstock::cli::run_command
  └─> transactions::buy()
        └─> Database transaction
              ├─> asset_repo::find_by_ticker() (asset must already exist)
              ├─> transaction_repo::insert_buy() (store with cents conversion)
              └─> Delete all dependent portfolio and per-asset snapshots
```

### `transaction sell`

```
rstock::cli::run_command
  └─> transactions::sell()
        └─> Database transaction
              ├─> Load the asset and its `(date, id)` ordered ledger
              ├─> Validate: net holdings >= sell quantity at sell date
              ├─> transaction_repo::insert_sell() (store with cents conversion)
              └─> Delete all dependent portfolio and per-asset snapshots
```

### `transaction dividend`

```
rstock::cli::run_command
  └─> transactions::dividend()
        └─> Database transaction
              ├─> Load the asset and its `(date, id)` ordered ledger
              ├─> transaction_repo::insert_dividend()
              └─> Delete all dependent portfolio and per-asset snapshots
```

### `transaction split`

```
rstock::cli::run_command
  └─> transactions::split()
        └─> Database transaction
              ├─> Load the asset and its `(date, id)` ordered ledger
              ├─> transaction_repo::insert_split()
              ├─> Delete the asset's split-adjusted price cache
              └─> Delete all dependent portfolio and per-asset snapshots
```

### `transaction export`

```
rstock::cli::run_command
  └─> export::export_transactions_csv()
        ├─> transaction_repo::find_all() (load all txs with asset info)
        └─> Write CSV file
```

### `analyze composition`

```
rstock::cli::run_command
  └─> composition::compute_composition()
        ├─> Load current Transaction ledger positions and Individual prices
        ├─> Aggregate classifications and look-through holdings without rebuilding NAV
        └─> Return composition result
  └─> display::print_composition()
```

### `analyze correlation matrix`

```
rstock::cli::run_command
  └─> analytics::compute_correlation_data()
        ├─> Derive current held assets from the Transaction ledger
        ├─> Request historical Base currency asset and benchmark series
        ├─> Compute pairwise Pearson correlations
        └─> Return N×N matrix
  └─> display::print_correlation_matrix()
```

### `analyze correlation rolling`

```
rstock::cli::run_command
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
rstock::cli::run_command
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
