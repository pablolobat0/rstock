# PRD: Deepen Market Data Module Interface

Status: needs-triage

## Problem Statement

The market data area has become a good Module with important behaviour, but its Interface exposes too many functionalities at once. Callers currently need to know too much about **Historical market data**, **Individual price**, **Live quote**, **Effective valuation date**, **Base currency** conversion, benchmark setup, FX lookup, stale-data policy, cache reads, and direct fetches.

This creates architectural friction. Understanding one market-data concept requires bouncing between several Modules. NAV, analytics, and portfolio display use different market-data rules, but those rules are exposed through a broad set of public functions rather than a small set of use-case-shaped Interfaces. This reduces Depth: the Module has a lot of implementation, but callers still carry much of the complexity.

The user wants the market data area reorganized into a deeper Rust Module. The aim is better Locality, more Leverage, clearer domain vocabulary, and tests that exercise the Module through its public Interface rather than through low-level helper functions.

## Solution

Reorganize market data into a Rust directory Module with private implementation submodules and a small public Interface. The public Interface will be use-case shaped rather than generic: valuation market data for NAV-like strict reads, correlation market data for analytics, **Individual price** for display-time values, and **Portfolio-relevant analysis** inputs that come from a **Market data source**.

The market data Module will also own source fetching. The current Python Morningstar scripts will be replaced by Rust implementation inside the market data Module. Yahoo Finance and Morningstar will become private source **Adapters** behind market-data Interfaces. Callers such as NAV, portfolio, analytics, composition, and fund analysis must not import or call Yahoo or Morningstar **Adapters** directly.

The valuation path will prepare **Historical market data**, persist allowed **Forward-filled market data**, calculate the **Effective valuation date**, return **Market data limitation** values, and provide strict valuation reads. Preparation functions may write cached **Historical market data**; read functions must not write.

The analytics path will prepare cache-first **Base currency** price series for correlation analysis. Analytics will not directly request native price series plus FX series. The market data Module will combine asset prices and required FX into **Base currency** series, return non-blocking **Market data limitation** values, and preserve the domain distinction that benchmark market data follows holdings-style historical availability rules but benchmark is not a holding.

The FX cache will use source-neutral `from_currency` and `to_currency` columns rather than provider-style pair strings. Provider-specific FX formatting, such as Yahoo ticker construction, belongs only inside source **Adapters**.

The display path will expose **Individual price** as the domain term for display-time per-asset pricing. It will keep **Live quote** behaviour separate from valuation market data, use snapshot fallback through a small fallback input value, and return **Market data limitation** values when display market data is stale beyond the accepted thresholds.

Analytics **Market data limitation** values will be displayed for both benchmark market data and Tracked asset series. The thresholds remain owned by market data: **Acceptable Morningstar lag** for funds/ETFs and **Completed weekday** stale-data cadence for stocks and FX. These limitations are non-blocking warnings; analytics should still produce results when enough aligned data exists.

## User Stories

1. As a portfolio owner, I want NAV to keep using **Historical market data**, so that NAV history remains reproducible.
2. As a portfolio owner, I want NAV valuation reads to be strict, so that missing required asset or FX market data is not silently ignored.
3. As a portfolio owner, I want NAV to keep using one **Effective valuation date**, so that portfolio valuation remains coherent across all holdings and required FX rates.
4. As a portfolio owner, I want **Live quote** values excluded from NAV, so that same-day display freshness never changes reproducible NAV history.
5. As a portfolio owner, I want **Forward-filled market data** to remain persisted only between source observations, so that calendar-day valuation remains reproducible without extrapolation.
6. As a portfolio owner, I want **Forward-filled market data** never to extend beyond the last source observation, so that unavailable market data is not invented.
7. As a portfolio owner, I want **Stale market data** to remain usable when cached data exists, so that temporary provider failures do not unnecessarily block valuation.
8. As a portfolio owner, I want **Market data limitation** values when stale data is actionable, so that I know when data freshness needs attention.
9. As a portfolio owner, I want normal **Acceptable Morningstar lag** to avoid warning noise, so that expected fund and ETF reporting delays are not treated as problems.
10. As a portfolio owner, I want excessive Morningstar lag to appear as a **Market data limitation**, so that unusual fund or ETF reporting delay is visible.
11. As a portfolio owner, I want stock stale-data warnings to use **Completed weekday** cadence, so that weekends do not create noisy warnings.
12. As a portfolio owner, I want FX stale-data warnings to use **Completed weekday** cadence, so that weekend gaps do not look like actionable FX problems.
13. As a portfolio owner, I want **Base currency** assets to use an implicit FX rate of 1.0, so that EUR-denominated assets do not require unnecessary FX data.
14. As a portfolio owner, I want non-**Base currency** assets converted with date-aligned FX market data, so that aggregate analytics and valuation remain in EUR.
15. As a portfolio owner, I want correlation analysis to use **Base currency** series, so that USD, GBP, and EUR assets are compared from the EUR portfolio perspective.
16. As a portfolio owner, I want correlation analysis to use aligned available series rather than one shared **Effective valuation date**, so that analytics can use available overlapping data without importing NAV semantics.
17. As a portfolio owner, I want correlation analysis to show **Market data limitation** values for stale Tracked asset series, so that analytics results do not hide stale inputs.
18. As a portfolio owner, I want correlation analysis to show **Market data limitation** values for benchmark market data, so that risk and correlation outputs do not hide stale benchmark inputs.
19. As a portfolio owner, I want analytics limitations to be non-blocking, so that useful analytics still render when enough aligned data exists.
20. As a portfolio owner, I want missing complete price history or missing required FX history to remain a clear error, so that analytics does not produce misleading results from absent data.
21. As a portfolio owner, I want insufficient aligned data to remain an analytics warning, so that freshness limitations and statistical insufficiency are not confused.
22. As a portfolio owner, I want benchmark market data to follow the same **Historical market data** availability rules as holdings, so that benchmark comparisons are consistent.
23. As a portfolio owner, I want benchmark market data to remain distinct from a holding, so that benchmark setup does not contaminate portfolio state.
24. As a portfolio owner, I want **Individual price** display to use a **Live quote** for stocks when available, so that current display can be more recent than NAV.
25. As a portfolio owner, I want **Individual price** display to use a **Live quote** for FX when available, so that non-EUR display values can be current.
26. As a portfolio owner, I want funds and ETFs not to use **Live quote** behaviour for display, so that their closing-price semantics remain clear.
27. As a portfolio owner, I want **Individual price** display to use snapshot fallback when current display data is unavailable, so that portfolio rows can still render.
28. As a portfolio owner, I want an **Individual price** that combines a stock **Live quote** with stale cached FX to carry a **Market data limitation**, so that mixed freshness is visible.
29. As a portfolio owner, I want **Individual price** naming in the codebase, so that implementation vocabulary matches the domain glossary.
30. As a maintainer, I want market data organized as a Rust directory Module, so that private implementation submodules are hidden behind one public Interface.
31. As a maintainer, I want callers to import market-data behaviour through the market data Module, so that internal file layout can change without spreading churn.
32. As a maintainer, I want the market data Interface split by use case, so that NAV, analytics, and display each learn only the facts they need.
33. As a maintainer, I want valuation preparation to be explicit, so that network calls, cache writes, and **Effective valuation date** calculation do not happen unexpectedly during read loops.
34. As a maintainer, I want read functions to be read-only, so that the Interface communicates when persistence can happen.
35. As a maintainer, I want preparation functions to own cache writes, so that **Historical market data** persistence and **Forward-filled market data** remain local to market data.
36. As a maintainer, I want analytics to go cache-first, so that direct fetch paths do not bypass **Forward-filled market data** and **Market data limitation** policy.
37. As a maintainer, I want rolling correlation to use the same cache-first market data path as other correlation analytics, so that analytics has one market-data behaviour.
38. As a maintainer, I want benchmark setup owned inside the analytics market-data path, so that analytics callers do not know the persistence compromise used for benchmark data.
39. As a maintainer, I want benchmark series returned separately from Tracked asset series, so that the Interface preserves the domain rule that benchmark is not a holding.
40. As a maintainer, I want correlation market data to return requested date range metadata, so that display can explain when available aligned data may end earlier.
41. As a maintainer, I want no shallow analysis-series wrapper if it only contains a vector, so that the Interface stays honest and minimal.
42. As a maintainer, I want a domain type alias for **Base currency** price series, so that callers know the series is already converted without learning FX details.
43. As a maintainer, I want **Market data limitation** values stored once on correlation market data, so that there is one source of truth for displayed warnings.
44. As a maintainer, I want **Market data limitation** values to remain self-describing, so that display can format asset and FX limitations without per-series duplication.
45. As a maintainer, I want public market-data result types in the shared model layer, so that service Modules own behaviour and model Modules own seam-crossing data shapes.
46. As a maintainer, I want private helper structs to stay inside market-data implementation submodules, so that implementation details do not become public Interface.
47. As a maintainer, I want **Individual price** fallback represented as one input value, so that callers cannot misorder native price, price date, and FX fallback arguments.
48. As a maintainer, I want **Individual price** to avoid accepting portfolio snapshot storage types directly, so that display pricing does not couple to snapshot persistence.
49. As a maintainer, I want no new cache Adapter seam yet, so that the design does not introduce a hypothetical seam with only one adapter.
50. As a maintainer, I want a stateful market data Module value, so that source **Adapters**, cache configuration, and test doubles are owned in one place.
51. As a maintainer, I want no prepared handle yet, so that the first refactor improves organization and Interface shape without changing call style broadly.
52. As a maintainer, I want tests to exercise the market data Module through public Interfaces, so that the Interface is the test surface.
53. As an implementation agent, I want low-level FX pair construction hidden, so that callers work with currencies and assets rather than provider pair strings.
54. As an implementation agent, I want lookup identity hidden, so that callers do not decide when to use ticker versus Morningstar code.
55. As an implementation agent, I want stale-data thresholds hidden, so that callers do not duplicate **Acceptable Morningstar lag** or **Completed weekday** policy.
56. As an implementation agent, I want direct fetching removed from public analytics market data, so that market-data rules are not bypassed.
57. As an implementation agent, I want old public helper functions removed or made private, so that the deletion test leaves complexity concentrated inside market data rather than spread across callers.
58. As a maintainer, I want source fetching owned by the market data Module, so that callers do not depend on Yahoo Finance, Morningstar, token handling, HTTP details, or source-specific JSON shapes.
59. As a maintainer, I want Yahoo Finance and Morningstar represented as private source **Adapters**, so that changing a **Market data source** later is local to market data.
60. As a maintainer, I want the Python Morningstar scripts deleted after equivalent Rust behaviour exists, so that rstock no longer depends on `uv`, Python packages, or subprocess JSON contracts for market data.
61. As a portfolio owner, I want Morningstar endpoints to remain the same during the Rust migration, so that fund and ETF **Historical market data**, holdings, and **Fund candidate** analysis keep current behaviour while the implementation changes.
62. As a maintainer, I want Morningstar token caching to use a private persistent cache at `~/.rstock/cache/morningstar_token.json`, so that repeated CLI commands make fewer requests to the pseudo-private token page without exposing cache details to callers.
63. As a maintainer, I want a `MarketDataSources` source Interface inside the market data Module, so that source observations can be injected into market data without exposing Yahoo Finance or Morningstar **Adapters** directly.
64. As a maintainer, I want `DefaultMarketDataSources` as the production source bundle, so that normal app wiring is explicit and future source-bundle changes have Locality.
65. As a maintainer, I want tests to define fake market data sources in `tests/common`, so that production code exposes no fake type while integration tests can vary source observations without network calls.
66. As a maintainer, I want source numeric series returned as `SourceObservation` values, so that prices and FX rates share one neutral dated-value shape without duplicating identical structs.
67. As a maintainer, I want `MarketDataSources` numeric observations sorted and deduplicated before they reach `MarketData`, so that source **Adapters** own source cleanup and `MarketData` can focus on cache coordination and market-data policy.
68. As a maintainer, I want the FX cache and repository Interface to use `from_currency` and `to_currency`, so that provider-specific pair strings do not leak into persistence or market-data callers.

## Implementation Decisions

- Build or modify one deep market data Module organized as a Rust directory Module with private submodules.
- The market data Module will expose a small public Interface through its module root.
- Internal submodules will be organized around valuation, analytics, **Individual price**, **Portfolio-relevant analysis** source data, market-data policy, and private source **Adapters**.
- Do not split cache or provider helper files immediately unless duplication proves they would be deep Modules.
- Public market-data result types live in the shared model layer.
- Private helper structs live inside the market-data implementation submodules.
- Rename the NAV preparation result to a valuation-oriented result because it represents prepared **Historical market data**, an **Effective valuation date**, and **Market data limitation** values, not NAV-only data.
- Rename display market data output to **Individual price** because that is the domain term.
- Add a small **Individual price** fallback input value instead of passing multiple fallback arguments.
- Keep **Individual price** independent from portfolio snapshot storage types.
- Replace the existing `PriceFetcher` public shape with market-data use-case Interfaces; any fetch-shaped functions should be private implementation details or private source Adapter methods.
- Implement private Yahoo Finance and Morningstar source **Adapters** inside the market data Module.
- Define a `MarketDataSources` trait inside `src/services/market_data/sources/` for raw source observations: price history, FX history, stock info, and fund data.
- Use `NaiveDate` at the `MarketDataSources` Interface for date inputs and outputs.
- Use a single neutral `SourceObservation { date, value }` type for source numeric series such as stock prices, fund prices, and FX rates.
- `MarketDataSources` numeric series must be sorted ascending by date and deduplicated before returning to `MarketData`; when duplicate source dates exist, the last source value wins.
- Expose source-shaped methods on `MarketDataSources`, such as stock price history, fund price history, exchange-rate history, stock info, and fund data, rather than passing `AssetType` into one generic price method.
- `MarketDataSources::exchange_rate_history` should accept source-neutral `from` and `to` currency values, not a provider-specific pair string; source **Adapters** own provider pair formatting.
- Replace the `daily_exchange_rates.pair` cache column with `from_currency` and `to_currency` through a migration that drops and recreates the FX cache table; existing cached FX rows do not need to be preserved.
- Update `exchange_rate_repo` to accept `from_currency` and `to_currency` instead of pair strings.
- Enforce uniqueness on `(from_currency, to_currency, date)` for the FX cache.
- Normalize currency values to uppercase inside `MarketData` before source calls and repository calls; repositories assume normalized currencies and remain pure data-access Modules.
- Validate FX currencies as three-letter alphabetic codes inside `MarketData` before source or repository calls.
- `MarketDataSources::exchange_rate_history` receives normalized uppercase currency codes.
- Same-currency FX conversion returns an implicit rate of 1.0 and bypasses both cache and `MarketDataSources`.
- Keep `fund_data(code, limit)` as the only fund-data method at both `MarketDataSources` and `MarketData` levels. Holdings-only callers should call `fund_data` and use its `holdings` field.
- Define `DefaultMarketDataSources` inside `src/services/market_data/sources/` as the production source bundle implementing `MarketDataSources`.
- Keep concrete Yahoo Finance and Morningstar **Adapters** private under `src/services/market_data/sources/`.
- Construct `DefaultMarketDataSources` in `main.rs` and inject it into `MarketData::new(Box<dyn MarketDataSources>)`.
- Inject `&MarketData` into NAV, portfolio, analytics, composition, and fund analysis Modules rather than injecting source **Adapters** into those Modules.
- Do not expose a generic public stock-history method just for monitor; monitor is not present in the current codebase.
- Preserve the current Morningstar chartservice and sal-service endpoints during the Rust migration.
- Add `reqwest` with rustls for the Rust Morningstar HTTP implementation.
- Implement Morningstar token scraping, JWT expiry parsing, persistent token caching, and `401` refresh in Rust as private Morningstar Adapter implementation details.
- Store the Morningstar token cache at `~/.rstock/cache/morningstar_token.json` as a private Morningstar Adapter implementation detail; do not expose public cache-path configuration.
- Delete the Python scripts directory and remove `uv` subprocess calls after Rust feature parity exists.
- Keep source **Adapters** inaccessible to NAV, portfolio, analytics, composition, and fund analysis callers; those Modules use only the market data Module root Interface.
- Preparation functions may fetch, cache, and persist **Historical market data**.
- Read functions must not write to the database.
- **Live quote** values must not be persisted as **Historical market data**.
- Valuation preparation remains explicit rather than auto-running during reads.
- Introduce a stateful market data Module value that owns source **Adapters** and cache configuration while keeping caller-facing operations in domain terms.
- Keep the first refactor metadata-returning rather than introducing a prepared handle.
- Valuation preparation calculates the **Effective valuation date** for NAV-like strict valuation.
- Valuation reads fail clearly when required asset price or required FX data is unavailable.
- Analytics market data is cache-first and should not expose a direct-fetch public path.
- Analytics market data returns **Base currency** price series, not native price series plus FX series.
- Analytics market data returns price series, not log return series; return math remains in the metrics Module.
- Correlation market data returns each series up to its own available data and lets analytics alignment determine comparable dates.
- Correlation market data does not force all series to one **Effective valuation date**.
- Correlation market data includes requested start and end dates so display can explain requested versus available periods.
- Correlation market data stores aggregate **Market data limitation** values once.
- Do not add a shallow analysis-series wrapper if it only wraps a vector.
- Use a domain type alias for **Base currency** price series to document that values are already converted to EUR.
- Benchmark analytics is represented separately from Tracked asset series in the correlation market data result.
- Benchmark lookup or creation is owned inside the analytics market-data path.
- Benchmark **Market data limitation** values are displayed when threshold-qualified.
- Tracked asset analytics **Market data limitation** values are displayed when threshold-qualified.
- Analytics **Market data limitation** values are non-blocking warnings.
- Missing required series data can remain blocking.
- Insufficient aligned data remains an analytics warning, not a **Market data limitation**.
- **Acceptable Morningstar lag** and **Completed weekday** stale-data thresholds stay inside market-data policy.
- Add one schema migration for the FX cache to replace provider-style pair storage with `from_currency` and `to_currency`.
- Repository Modules remain data-access Modules.
- Existing price-fetching adapter shape is replaced by private source **Adapters** inside market data.
- Keep implementation documentation minimally correct by removing false Python, `uv`, `RSTOCK_SCRIPTS_DIR`, and old `PriceFetcher` references after the Rust source **Adapter** is active; broader architecture documentation is deferred to a later documentation pass.
- `CONTEXT.md` already records that correlation analysis uses aligned available **Base currency** series and does not force one **Effective valuation date**.

## Testing Decisions

- Good tests should exercise external behaviour through market data, NAV rebuild, analytics, and display Interfaces rather than private helper functions.
- Market data valuation tests should cover explicit preparation followed by strict read behaviour.
- Market data valuation tests should cover **Effective valuation date** calculation across multiple assets and required FX rates.
- Market data valuation tests should cover persisted **Forward-filled market data** between source observations.
- Market data valuation tests should cover no **Forward-filled market data** beyond the last source observation.
- Market data valuation tests should cover **Base currency** assets using implicit FX rate 1.0.
- Market data valuation tests should cover non-**Base currency** assets requiring FX market data.
- Market data valuation tests should cover missing required asset price data failing clearly.
- Market data valuation tests should cover missing required FX data failing clearly.
- Market data valuation tests should cover missing Morningstar code for required fund or ETF data failing clearly.
- Market data policy tests should cover **Acceptable Morningstar lag** not creating warning noise within the accepted threshold.
- Market data policy tests should cover excessive fund or ETF reporting lag returning a **Market data limitation**.
- Market data policy tests should cover stock stale-data warnings using **Completed weekday** cadence.
- Market data policy tests should cover FX stale-data warnings using **Completed weekday** cadence.
- **Individual price** tests should cover stock **Live quote** use.
- **Individual price** tests should cover FX **Live quote** use.
- **Individual price** tests should cover funds and ETFs not using **Live quote** display behaviour.
- **Individual price** tests should cover snapshot fallback through the fallback input value.
- **Individual price** tests should cover a stock **Live quote** combined with stale cached FX returning a **Market data limitation**.
- Analytics market data tests should cover cache-first **Base currency** series construction for Tracked assets.
- Analytics market data tests should cover shared FX preparation for multiple non-**Base currency** Tracked assets.
- Analytics market data tests should cover benchmark market data being returned separately from Tracked asset series.
- Analytics market data tests should cover benchmark setup being hidden from analytics callers.
- Analytics tests should cover correlation using aligned available **Base currency** series rather than one shared **Effective valuation date**.
- Analytics tests should cover non-blocking **Market data limitation** values being carried into correlation matrix results.
- Analytics tests should cover non-blocking **Market data limitation** values being carried into rolling correlation results.
- Display tests should cover market-data limitation formatting reused by portfolio and analytics output.
- Display tests should verify benchmark and Tracked asset analytics limitations are shown when present.
- Existing daily price tests are prior art for cache behaviour and forward-fill behaviour.
- Existing NAV tests are prior art for strict valuation and snapshot rebuild behaviour.
- Existing correlation tests are prior art for matrix and rolling correlation outputs.
- Existing portfolio summary tests are prior art for **Individual price** consumption in portfolio rows.
- Tests must use in-memory SQLite, dummy tickers, and fake `MarketDataSources` in `tests/common`.
- Tests must not make network calls.
- Tests should replace the existing mock price fetcher with market-data/source test doubles that exercise the public market-data Interface.
- Test doubles should live in `tests/common`, implement `MarketDataSources`, and be injected into `MarketData::new(...)`, not into NAV, portfolio, analytics, composition, or fund analysis directly.
- Rust Morningstar parsing tests should use static sample payloads for chartservice time series and sal-service holdings/fund data.

## Out of Scope

- Changing NAV unitization rules is out of scope.
- Changing **Effective valuation date** semantics for NAV is out of scope.
- Introducing configurable **Base currency** is out of scope.
- Introducing exchange-specific market calendars is out of scope.
- Introducing a new cache Adapter seam is out of scope.
- Introducing public source Adapter Interfaces for callers outside market data is out of scope.
- Refactoring fund look-through composition is out of scope.
- Refactoring the Transaction ledger is out of scope.
- Changing repository persistence schema is out of scope except for replacing the FX cache pair column with `from_currency` and `to_currency`.
- Persisting **Market data limitation** values is out of scope.
- Changing Morningstar endpoints or Yahoo Finance behaviour is out of scope except where required to preserve current behaviour in Rust.
- Keeping any Python script or `uv` fallback path after Rust feature parity is out of scope.
- Moving log return or correlation math into market data is out of scope.
- Implementation issue slicing is tracked under `.scratch/deepen-market-data-module-interface/issues/`.

## Further Notes

- This PRD refines the earlier market-data deepening work with a stronger focus on Rust module organization and use-case-shaped Interfaces.
- The primary architectural goal is Depth: callers should get valuation data, correlation market data, or **Individual price** through small Interfaces without learning provider lookup identity, FX pair construction, cache mechanics, or stale-data thresholds.
- The primary maintainability goal is Locality: market-data policy, cache preparation, **Base currency** conversion, and **Market data limitation** decisions should change in one place.
- The user explicitly prefers a Rust directory Module with private submodules and public re-exports over many top-level service Modules.
- The user explicitly prefers no shallow wrapper type for analytics series when a domain type alias communicates the concept adequately.
- The user explicitly chose cache-first analytics market data and non-blocking analytics **Market data limitation** display.
- The user explicitly chose to merge source fetching into the market data Module rather than keep `PriceFetcher` as a separate public Module.
- The user explicitly wants the Python Morningstar implementation deleted and replaced with Rust while preserving the current Morningstar endpoints for now.
- The user explicitly wants all Python scripts deleted after Rust feature parity, not only the scripts currently called by Rust.
- The user explicitly wants Yahoo Finance and Morningstar as private source **Adapters** so a future **Market data source** change has Locality inside market data.
- The user explicitly chose a private persistent Morningstar token cache at `~/.rstock/cache/morningstar_token.json` to reduce repeated requests across back-to-back CLI commands.
- The user explicitly chose not to expose Morningstar token cache path configuration and not to require direct token-cache integration tests.
- The user explicitly changed the market data Module from stateless functions to a stateful Module value because source **Adapters** and cache configuration now belong inside market data.
- The user explicitly chose a `MarketDataSources` source Interface injected into `MarketData`, with `DefaultMarketDataSources` built in `main.rs`, because it keeps market-data dependencies explicit without leaking Yahoo Finance or Morningstar **Adapters** to other Modules.
- The user explicitly chose to keep fake market data sources in `tests/common` rather than exporting fake types from production code.
- The user explicitly chose `SourceObservation` as the neutral dated numeric source type for prices and FX rates.
- The user explicitly chose sorted and deduplicated `MarketDataSources` observations as an Interface invariant, with duplicate source dates resolved by keeping the last source value.
- The user explicitly chose `exchange_rate_history(from, to, start, end)` over passing provider-specific pair strings, so future **Market data source** formatting changes stay local to source **Adapters**.
- The user explicitly chose to migrate the FX cache and repository Interface from provider-style pair strings to `from_currency` and `to_currency`, without preserving existing cached FX rows.
- The user explicitly chose uniqueness on `(from_currency, to_currency, date)` for the FX cache.
- The user explicitly chose currency normalization as a `MarketData` responsibility rather than repository responsibility.
- The user explicitly chose three-letter alphabetic FX currency validation inside `MarketData`.
- The user explicitly chose same-currency FX conversion as an implicit 1.0 rate with no cache or source call.
- The user explicitly chose to simplify both the source Interface and public market data Interface to `fund_data(code, limit)` only, with holdings-only callers using the returned `holdings` field.
- ADR-0001 records the market data source coordination and FX cache decisions for future architecture reviews.
