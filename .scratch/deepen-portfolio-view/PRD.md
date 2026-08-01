# PRD: Deepen Portfolio View Assembly

Status: ready-for-agent

## Problem Statement

The **Portfolio view** currently mixes current **Transaction ledger** inventory, snapshot readiness, NAV rebuilding, **Individual price** preparation, cost and dividend calculations, aggregate values, returns, risk metrics, and **Market data limitation** values in one broad result. A second entry point reuses that same result for current positions by leaving many NAV and metric fields empty. Callers must therefore know which fields are meaningful and when NAV must be rebuilt before requesting another outcome.

Current holding economics are also duplicated. Performance positions and **Monetary holding** values scan the same **Transaction ledger** through different implementations. They disagree when historical FX is unavailable: one path substitutes a current FX rate, while the other marks Base currency cost facts unavailable. Performance positions require every price-dependent fact and are omitted when no **Individual price** exists, while **Monetary holding** values remain visible with unavailable facts. This contradicts the domain rule that the **Transaction ledger** is the source of truth for current inventory.

The result can mislead a portfolio owner. A newly bought holding may disappear from the **Portfolio view**; a partial aggregate can look complete; current FX can be presented as historical cost conversion; and lifetime dividends are currently mixed into gain/loss for units that remain after partial sells. Real-clock access also makes current-inventory and rebuild scenarios harder to test deterministically.

## Solution

Deepen the portfolio module around one interface with two domain-shaped outcomes: focused current positions and the full **Portfolio view**. Both outcomes will use one internal projection of current **Transaction ledger** inventory. The full **Portfolio view** will enrich current positions with synchronized NAV, returns, and risk facts; current-position consumers such as portfolio composition will not rebuild NAV or receive a broad result filled with unrelated unavailable fields.

Use one availability-aware position representation for performance positions and **Monetary holding** values while retaining separate collections because Monetary holdings do not participate in portfolio performance. Every currently held **Tracked asset** remains visible. Quantity and available ledger facts remain present even when price, historical FX, current value, or dependent facts are unavailable.

Apply one canonical financial policy across all holdings. **Average cost** is weighted-average Base currency acquisition cost including buy fees; sells remove cost proportionally and splits change quantity without changing total cost. **Open-position gain/loss** is current value minus remaining Average cost. Dividends remain separate lifetime income and do not contribute to Open-position gain/loss. Missing transaction-date historical FX makes affected cost, dividend, and dependent facts unavailable rather than substituting current or later FX.

Every aggregate is complete or unavailable. Known per-position facts remain visible, but no partial sum is presented as a complete total. The informational Total value may combine different **Individual price** dates, while NAV remains synchronized at one **Effective valuation date**.

Move NAV readiness ownership to the NAV module. Every NAV consumer requests readiness there; current-position and asset-series consumers do not. Introduce an injected clock seam with system and fixed-test adapters so current inventory, market-data windows, and NAV rebuild cutoffs are deterministic without making every caller pass a date.

## User Stories

1. As a portfolio owner, I want every currently held **Tracked asset** shown in the **Portfolio view**, so that my current inventory matches the **Transaction ledger**.
2. As a portfolio owner, I want a holding bought after the latest **Effective valuation date** shown immediately, so that stale NAV does not hide current inventory.
3. As a portfolio owner, I want an unpriced holding to remain visible, so that unavailable market data is not confused with no holding.
4. As a portfolio owner, I want an unpriced holding's quantity shown, so that facts derived from the **Transaction ledger** remain useful.
5. As a portfolio owner, I want available cost facts shown when an **Individual price** is unavailable, so that one unavailable fact does not erase independent facts.
6. As a portfolio owner, I want unavailable price-dependent facts represented explicitly, so that missing values are not displayed as zero.
7. As a portfolio owner, I want performance positions and **Monetary holding** values to follow the same availability rules, so that classification does not change the meaning of missing data.
8. As a portfolio owner, I want performance positions and **Monetary holding** values kept in separate collections, so that portfolio performance excludes Monetary holdings.
9. As a portfolio owner, I want **Average cost** calculated consistently across all current holdings, so that the same ledger history has one interpretation.
10. As a portfolio owner, I want buy fees included in **Average cost**, so that remaining invested cost reflects acquisition expense.
11. As a portfolio owner, I want partial sells to remove weighted-average cost proportionally, so that remaining inventory retains its weighted-average cost.
12. As a portfolio owner, I want splits to change quantity without changing total cost, so that **Average cost** adjusts correctly after a split.
13. As a portfolio owner, I want **Open-position gain/loss** to compare current value with remaining Average cost, so that it describes only units I still hold.
14. As a portfolio owner, I want dividends reported separately from **Open-position gain/loss**, so that lifetime income is not assigned to units remaining after a partial sell.
15. As a portfolio owner, I want dividend fees reflected in lifetime dividend income, so that the reported income is net of recorded fees.
16. As a portfolio owner, I want realized gains from sold units excluded from **Open-position gain/loss**, so that open and disposed inventory are not conflated.
17. As a portfolio owner, I want missing historical FX to make Base currency cost facts unavailable, so that current FX is not presented as historical conversion.
18. As a portfolio owner, I want the latest FX rate on or before each transaction date used for Base currency cost and dividend facts, so that historical conversion follows available historical evidence.
19. As a portfolio owner, I want EUR transactions to use the implicit Base currency FX rate of 1.0, so that they do not depend on external FX data.
20. As a portfolio owner, I want current value to remain available when historical cost FX is unavailable, so that independent current valuation remains useful.
21. As a portfolio owner, I want **Open-position gain/loss** unavailable when current value or remaining cost is unavailable, so that it is never inferred from incomplete inputs.
22. As a portfolio owner, I want each aggregate either complete or unavailable, so that a partial sum is not presented as my whole portfolio.
23. As a portfolio owner, I want known per-position facts retained when an aggregate is unavailable, so that one missing holding does not erase valid detail.
24. As a portfolio owner, I want the current performance-holdings total unavailable when any performance holding cannot be valued, so that it represents all current performance holdings or none.
25. As a portfolio owner, I want the Monetary holding subtotal unavailable when any Monetary holding cannot be valued, so that it represents all Monetary holdings or none.
26. As a portfolio owner, I want Total value unavailable when either performance or Monetary holding subtotal is unavailable, so that Total value is never a partial portfolio estimate.
27. As a portfolio owner, I want remaining invested cost, dividends, and Open-position gain/loss aggregates to follow the same completeness rule, so that aggregate semantics are predictable.
28. As a portfolio owner, I want the informational Total value to use each holding's latest available **Individual price**, so that it reflects the freshest current estimate available for each holding.
29. As a portfolio owner, I want each **Individual price** date retained, so that I can see when the informational Total value combines different dates.
30. As a portfolio owner, I want NAV to remain synchronized at one **Effective valuation date**, so that current display freshness does not compromise reproducible performance measurement.
31. As a portfolio owner, I want the distinction between informational Total value and NAV clear, so that I do not interpret mixed-date current values as synchronized NAV.
32. As a portfolio owner, I want a stock **Individual price** to use a **Live quote** when available, so that current display can be newer than NAV.
33. As a portfolio owner, I want an ETF **Individual price** to use a **Live quote** when its **Market data source** supplies one, so that ETF display freshness follows source capability rather than vehicle type alone.
34. As a portfolio owner, I want an ETF without a supported Live quote to use its latest **Historical market data**, so that it remains displayable without invented freshness.
35. As a portfolio owner, I want mutual funds to continue using closing-price semantics, so that their **Individual price** is not presented as a Live quote.
36. As a portfolio owner, I want NAV/history limitations separated from current performance-position limitations, so that a current quote problem does not imply invalid historical NAV.
37. As a portfolio owner, I want **Monetary holding** limitations reported separately, so that they do not imply a limitation on portfolio performance.
38. As a portfolio owner, I want affected position limitations retained beside each position, so that I can identify which holding caused an unavailable fact.
39. As a portfolio owner, I want portfolio composition to use current **Transaction ledger** inventory, so that analysis reflects what I hold now rather than what I held at the Effective valuation date.
40. As a portfolio owner, I want value-dependent composition unavailable when any included holding lacks an **Individual price**, so that composition weights are not calculated from a partial portfolio.
41. As a portfolio owner, I want requesting composition not to rebuild NAV, so that current-position analysis performs only the preparation it needs.
42. As a portfolio owner, I want every feature that consumes NAV history to ensure NAV is ready, so that NAV-based analysis does not depend on running `get` first.
43. As a portfolio owner, I want asset-series correlation to prepare the historical Base currency series it needs without unnecessary current-position valuation, so that distinct analysis clocks remain distinct.
44. As a CLI user, I want human output to mark unavailable facts clearly, so that missing data is not rendered as a plausible number.
45. As a JSON consumer, I want unavailable scalar facts serialized as `null`, so that missing data remains distinct from zero.
46. As a JSON consumer, I want performance and Monetary positions to share the same field semantics, so that both collections can be interpreted consistently.
47. As a JSON consumer, I want structured limitation scopes preserved, so that automation can distinguish NAV, current performance-position, and Monetary holding constraints.
48. As a maintainer, I want one portfolio interface to be the test surface, so that tests verify caller-visible behavior rather than private calculation helpers.
49. As a maintainer, I want focused current positions distinct from the full **Portfolio view**, so that callers do not receive unrelated nullable NAV and risk fields.
50. As a maintainer, I want one internal **Transaction ledger** projection for all holdings, so that quantity, cost, dividends, splits, and sells have Locality.
51. As a maintainer, I want the portfolio module to prepare **Individual price** data once for open holdings, so that callers do not coordinate per-position market-data steps.
52. As a maintainer, I want NAV readiness owned by the NAV module, so that every NAV consumer gets the same rebuild behavior without leaked call order.
53. As a maintainer, I want a fixed clock adapter in tests, so that current dates and rebuild cutoffs are deterministic.
54. As a maintainer, I want a system clock adapter in production, so that normal callers do not pass today's date through every interface call.
55. As a maintainer, I want the clock seam shared by portfolio and NAV behavior, so that one invocation has a consistent meaning of today.
56. As a maintainer, I want no public pure ledger helper created only for testing, so that the portfolio interface remains the test surface.
57. As a maintainer, I want existing no-network fake market data to remain usable, so that portfolio scenarios are deterministic and isolated from external sources.
58. As a maintainer, I want the market data module to retain source coordination under ADR-0001, so that portfolio callers do not learn source adapter details.
59. As an implementation agent, I want result invariants expressed in domain terms, so that unavailable values, totals, and limitations are implemented consistently.
60. As an implementation agent, I want obsolete shallow entry points removed after callers migrate, so that the old call-order and nullable-result conventions cannot be reused accidentally.

## Implementation Decisions

- Deepen one portfolio module rather than layering a new pass-through module over the current implementation.
- The portfolio module has one interface with two domain-shaped outcomes: focused current positions and the full **Portfolio view**.
- The focused current-positions outcome contains current inventory, position facts, complete-or-unavailable aggregates, and current-position **Market data limitation** values. It does not contain NAV, returns, or risk facts.
- The full **Portfolio view** builds on the same current-position implementation and adds NAV, Effective valuation date, return, risk, and NAV/history limitation facts.
- Current positions do not trigger NAV rebuilding. The full Portfolio view requests NAV readiness because it consumes NAV history.
- Move the ensure-current behavior for NAV history into the NAV module. Every NAV consumer uses that behavior; callers do not invoke a portfolio-specific rebuild prerequisite.
- Keep NAV unitization and **Effective valuation date** semantics unchanged.
- Introduce a clock interface accepted by the modules that need a consistent current date. Provide a system-time adapter for production and a fixed-time adapter for tests. These two adapters make the seam real.
- Do not make callers pass an as-of date on every portfolio request.
- Use one internal projection of ordered **Transaction ledger** entries for current holdings. The implementation derives quantity, remaining weighted-average cost, lifetime dividends, and the availability of each fact once per Tracked asset.
- Exclude future-dated Transaction ledger entries relative to the injected clock.
- Use one availability-aware position representation for performance positions and **Monetary holding** values.
- Preserve separate performance-position and Monetary-position collections in the Portfolio view and JSON output.
- Position identity, classification, quantity, and any independently available ledger facts remain present even when valuation facts are unavailable.
- Price, price date, current value, remaining invested cost, dividends, Open-position gain/loss, and percentage values are independently nullable when their required inputs are unavailable.
- **Average cost** is weighted-average Base currency acquisition cost per currently held unit, includes buy fees, removes cost proportionally on sells, and adjusts units without changing total cost on splits.
- **Open-position gain/loss** is current Base currency value minus remaining weighted-average cost. Dividends and realized gains are excluded.
- Dividends remain a separate lifetime net-income fact for each Tracked asset and are not allocated to remaining units after partial sells.
- Historical Base currency cost and dividend conversion uses the latest FX rate on or before each transaction date.
- Missing historical FX does not fall back to current or later FX. It makes affected cost, dividend, and dependent facts unavailable and produces an appropriate **Market data limitation**.
- Current value can remain available when historical cost or dividend facts are unavailable.
- Every aggregate is complete across all holdings in its scope or unavailable. Do not return partial sums as complete aggregate values.
- Keep known per-position and independent subtotal facts visible when another aggregate is unavailable.
- Total value is unavailable when either the performance-position current-value subtotal or Monetary holding subtotal is unavailable.
- The informational Total value may combine different **Individual price** dates and remains distinct from NAV.
- Keep NAV/history, current performance-position, and Monetary holding limitation collections distinct at the Portfolio view interface.
- Continue using the market data module Interface for **Individual price**, historical FX, and limitation policy. Do not expose Yahoo Finance or Morningstar adapters or move source coordination out of market data.
- ETF Live quote behavior is capability-based: use a Live quote when the existing Market data source path can supply one; otherwise use the latest Historical market data. This spec does not require introducing a new source or new lookup metadata solely to make every ETF live-priced.
- Portfolio composition consumes the focused current-positions outcome and does not explicitly rebuild NAV.
- Portfolio composition uses current Transaction ledger inventory. If any included position lacks required current valuation, value-dependent composition facts and aggregates are unavailable rather than computed from a subset.
- Correlation modules request their actual prerequisite: historical Base currency asset series for asset correlation, and ensured NAV history only for portfolio-NAV correlation.
- Replace the broad current-position use of the full Portfolio view result with focused result types. Do not signal result intent by filling unrelated fields with null values.
- Human output labels gain/loss as **Open-position gain/loss** or an unambiguous short form and displays dividends separately.
- JSON output uses `null` for unavailable scalar facts and preserves the separate position collections and limitation scopes.
- JSON schemas remain unversioned and carry no backward-compatibility guarantee, so field and nullability changes can directly reflect the corrected domain model.
- No database schema or migration changes are required.
- Update architecture and convention documentation where it describes old portfolio result, rebuild ownership, gain/loss, availability, or ETF Individual price behavior.
- Remove obsolete public rebuild orchestration and shallow current-position entry points after all callers migrate.

## Testing Decisions

- The primary and highest test seam is the portfolio module interface. Tests exercise both focused current positions and the full **Portfolio view** rather than calling private ledger, aggregation, or valuation helpers.
- A good test verifies caller-visible domain behavior: which holdings appear, which facts are available, how values are calculated, when aggregates become unavailable, which limitations are returned, and whether NAV work occurs when required.
- Tests should not assert private helper decomposition, repository call order, internal collection types, or the number of internal calculation passes.
- Use in-memory SQLite through the existing database test setup, dummy Tracked asset identifiers, fake `MarketDataSources`, and a fixed clock adapter. Tests must not make network calls.
- Existing portfolio summary tests are prior art for constructing current holdings, snapshots, splits, Individual prices, Monetary holdings, and market-data limitations.
- Existing NAV tests are prior art for rebuild behavior, Effective valuation date, dividends, splits, and strict Historical market data valuation.
- Existing composition tests are prior art for current portfolio allocation and look-through outcomes.
- Existing correlation and Fund candidate tests are prior art for distinguishing asset-series requirements from portfolio-NAV history requirements.
- Preserve and adapt the existing scenario proving that a holding bought after the Effective valuation date appears with current Transaction ledger quantity and split-adjusted Average cost.
- Rewrite the existing scenario that expects an unpriced performance holding to be omitted. It must instead assert that the holding remains visible with unavailable price-dependent facts and complete-or-unavailable aggregates.
- Generalize existing Monetary holding missing-price coverage to prove that performance and Monetary positions use the same availability semantics.
- Test an empty Transaction ledger through both focused current positions and the full Portfolio view.
- Test that future-dated transactions are excluded relative to the fixed clock for both performance and Monetary holdings.
- Test weighted-average cost across multiple buys at different prices and fees.
- Test a partial sell removes remaining cost proportionally.
- Test a split changes quantity and Average cost per unit without changing total remaining cost.
- Test that dividends remain separate and do not change Open-position gain/loss.
- Test a partial sell after a dividend to prove lifetime dividend income is not attributed to remaining units through gain/loss.
- Test that realized gains from sells do not enter Open-position gain/loss.
- Test EUR holdings use implicit FX 1.0 for historical cost and dividends.
- Test non-EUR holdings use the latest FX rate on or before each transaction date.
- Test missing historical cost FX does not fall back to a current FX rate.
- Test missing historical dividend FX makes dividend and dividend aggregates unavailable without erasing quantity or an independently available current value.
- Test independently unavailable facts propagate only to dependent position facts.
- Test any unavailable performance-position current value makes the performance current-value aggregate unavailable.
- Test any unavailable Monetary holding current value makes the Monetary subtotal unavailable.
- Test either unavailable subtotal makes Total value unavailable.
- Test remaining-cost, dividend, Open-position gain/loss, and percentage aggregates follow the same complete-or-unavailable invariant.
- Test known position facts remain visible when aggregates are unavailable.
- Test the informational Total value can combine Individual prices with different dates while retaining each price date.
- Test the full Portfolio view retains synchronized NAV and its Effective valuation date independently from mixed-date current values.
- Test NAV/history, current performance-position, and Monetary holding limitations remain in separate scopes.
- Test focused current positions do not create or rebuild NAV history.
- Test the full Portfolio view ensures stale or absent NAV history through the NAV module.
- Test a NAV-based analysis ensures NAV readiness without requiring `get` to run first.
- Test composition consumes current holdings bought after the latest Effective valuation date.
- Test composition does not rebuild NAV.
- Test value-dependent composition becomes unavailable when an included holding has no Individual price rather than silently excluding that holding.
- Test system-time behavior indirectly only through production wiring; domain scenarios use the fixed clock adapter.
- Test human display renders unavailable facts clearly and keeps dividends separate from Open-position gain/loss.
- Test JSON structurally through `serde_json::Value`, including shared position field semantics, `null` unavailable values, separate collections, complete-or-unavailable aggregates, and separate limitation scopes.
- Run `cargo fmt && cargo clippy -- -D warnings` and `cargo test` as final verification.

## Out of Scope

- Changing NAV unitization is out of scope.
- Changing the rule that NAV uses one **Effective valuation date** is out of scope.
- Using Live quotes or mixed-date prices in NAV is out of scope.
- Adding realized gain/loss reporting is out of scope.
- Adding FIFO, LIFO, tax-lot selection, or jurisdiction-specific tax reporting is out of scope.
- Adding a combined lifetime total-return fact is out of scope.
- Changing Transaction ledger mutation, validation, import, or invalidation workflows is out of scope except where existing records are read by the portfolio module.
- Adding a database table, column, or migration is out of scope.
- Persisting current position projections or **Market data limitation** values is out of scope.
- Introducing a new Market data source, ETF symbol mapping, or lookup metadata solely to provide ETF Live quotes is out of scope.
- Reopening ADR-0001 market-data source coordination is out of scope.
- Exposing Yahoo Finance or Morningstar adapters outside the market data module is out of scope.
- Redesigning fund analysis or fund comparison is out of scope.
- Redesigning general CLI command dispatch or the human/JSON output module is out of scope.
- Providing backward compatibility for the unversioned JSON Portfolio view schema is out of scope.
- Refactoring unrelated repository modules is out of scope.
- Reorganizing the entire test suite is out of scope beyond the portfolio, NAV-consumer, composition, correlation, display, and JSON scenarios required by this behavior.
- Creating an ADR is out of scope because the accepted domain rules are recorded in `CONTEXT.md` and the module/clock design is reversible.

## Further Notes

- The accepted testing seam was confirmed during the grilling session: portfolio behavior should be exercised through the portfolio module interface with in-memory SQLite, fake market data, and fixed time. A public pure ledger helper should not be introduced for tests.
- The design deliberately separates three clocks: current Transaction ledger inventory through the injected clock, each holding's latest Individual price date for the informational Total value, and the synchronized Effective valuation date for NAV.
- ADR-0001 remains authoritative: the market data module owns source coordination, cache policy, Base currency conversion behavior, and private source adapters.
- The deletion test supports retaining and deepening the portfolio module. Removing it would spread ledger projection, Individual price preparation, aggregate completeness, NAV enrichment, and limitation policy across composition and CLI callers.
- The dedicated current-positions outcome is part of the portfolio module interface, not a separate public inventory module. This provides Leverage to the Portfolio view and composition while keeping Locality for current-holding rules.
- Existing behavior that substitutes current FX for missing historical transaction FX is intentionally rejected.
- Existing behavior that adds dividends to gain/loss is intentionally rejected. Dividends remain visible as a separate lifetime income fact.
- Existing behavior that omits unpriced performance holdings is intentionally rejected.
- The domain glossary now defines Portfolio view, Average cost, and Open-position gain/loss and records availability, aggregate completeness, composition, ETF Individual price, FX, and limitation-scope rules for this work.
