# PRD: Deepen Historical Market Data Architecture

Status: needs-triage

## Problem Statement

The earlier Historical market data implementation improved NAV correctness and market-data behaviour, but the resulting code still has architectural friction. Historical market data, Individual price display, Live quote fallback, Effective valuation date calculation, Base currency conversion, FX identity, and Market data limitation policy are still too close together from a Module-depth perspective.

This makes the code harder to navigate, harder to test through stable Interfaces, and easier to misuse. Callers can still learn details that should be Implementation concerns, such as provider-specific FX pair strings, source-versus-cache mechanics, and when a missing required valuation should be an error versus a display fallback.

The user wants a follow-up architecture refactor that turns the current market-data area into deeper Modules. The goal is not to change the product behaviour broadly, but to improve Locality, Leverage, and testability while preserving the project’s domain rules for NAV, Historical market data, Individual price, Live quote, Effective valuation date, Forward-filled market data, Base currency, Acceptable Morningstar lag, Completed weekday, and Market data limitation.

## Solution

Split the mixed market-data implementation into two domain-named Modules: Historical market data and Individual price.

Historical market data will be a deeper Module exposed through stateless service functions. It will prepare required Historical market data, infer required FX from supplied assets, calculate the Effective valuation date, preserve persisted Forward-filled market data, hide provider-specific FX identity, provide strict valuation reads for required NAV/benchmark data, and return only actionable Market data limitation values.

Individual price will be a separate Module exposed through stateless service functions. It will own display-time behaviour: Live quote use for stocks and FX, no Live quote use for funds/ETFs, snapshot fallback for display continuity, and actionable Market data limitation values for display freshness problems.

NAV rebuild will fail clearly when required Historical market data valuation is absent. Benchmark analytics will own benchmark asset lookup or creation, then pass that asset through Historical market data preparation. User-facing warning text will move out of market-data Modules and into presentation code.

## User Stories

1. As a portfolio owner, I want NAV to keep using one Effective valuation date, so that portfolio performance is calculated from coherent Historical market data.
2. As a portfolio owner, I want NAV to use Historical market data only, so that NAV history remains reproducible.
3. As a portfolio owner, I want Live quote values excluded from NAV, so that same-day display values never affect NAV.
4. As a portfolio owner, I want NAV to fail clearly when required asset valuation data is absent, so that no holding is silently omitted.
5. As a portfolio owner, I want NAV to fail clearly when required FX valuation data is absent, so that non-Base currency assets are never converted with hidden assumptions.
6. As a portfolio owner, I want Stale market data to remain usable when cached Historical market data exists, so that temporary source failures do not block NAV unnecessarily.
7. As a portfolio owner, I want Stale market data to move the Effective valuation date earlier, so that NAV does not pretend to be more current than the required data supports.
8. As a portfolio owner, I want missing market data and Stale market data to remain separate concepts, so that hard failures and earlier valuation dates are not confused.
9. As a portfolio owner, I want Forward-filled market data to remain persisted between source observations, so that completed non-trading dates remain reproducible.
10. As a portfolio owner, I want Forward-filled market data never to extend beyond the last source observation, so that unavailable prices are not extrapolated.
11. As a portfolio owner, I want Historical market data to ignore same-day source values, so that only completed dates enter NAV and benchmark calculations.
12. As a portfolio owner, I want EUR to remain the Base currency, so that NAV and aggregate values continue to be expressed consistently.
13. As a portfolio owner, I want Base currency assets to use an implicit FX rate of 1.0, so that EUR holdings do not require unnecessary FX data.
14. As a portfolio owner, I want non-Base currency assets to require Historical market data for FX conversion, so that EUR valuation is explicit.
15. As a portfolio owner, I want FX Market data limitation values to refer to the non-Base currency, so that warnings use domain language rather than provider pair strings.
16. As a portfolio owner, I want stocks to keep using ticker lookup for Historical market data, so that stock pricing behaviour is preserved.
17. As a portfolio owner, I want funds and ETFs to keep using Morningstar code lookup for Historical market data, so that fund and ETF pricing behaviour is preserved.
18. As a portfolio owner, I want funds and ETFs without Morningstar codes to fail clearly when required for NAV, so that asset metadata problems are visible.
19. As a portfolio owner, I want Acceptable Morningstar lag to limit NAV without creating a Market data limitation, so that normal fund and ETF reporting delay does not create warning noise.
20. As a portfolio owner, I want excessive Morningstar lag to create an actionable Market data limitation, so that unusual reporting delay is visible.
21. As a portfolio owner, I want stock and FX stale-data policy to continue using Completed weekday cadence, so that weekends do not create noisy warnings.
22. As a portfolio owner, I want no exchange-specific market-calendar Module in this refactor, so that the architecture stays focused and simple.
23. As a portfolio owner, I want Individual price display to be able to show newer values than NAV, so that current display remains useful.
24. As a portfolio owner, I want Individual price display to use Live quote values for stocks when available, so that stock display can be current.
25. As a portfolio owner, I want Individual price display to use Live quote FX when available, so that non-Base currency display can be current.
26. As a portfolio owner, I want funds and ETFs to avoid Live quote display behaviour, so that their closing-price semantics remain clear.
27. As a portfolio owner, I want Individual price display to preserve snapshot fallback, so that portfolio rows can still render when current display data is unavailable.
28. As a portfolio owner, I want Individual price display to return actionable Market data limitation values, so that stale display inputs are visible when action is needed.
29. As a portfolio owner, I want a stock Live quote combined with stale cached FX to produce a Market data limitation, so that mixed freshness is not hidden.
30. As a portfolio owner, I want benchmark market data to follow the same Historical market data rules as holdings, so that benchmark analytics are comparable with NAV.
31. As a portfolio owner, I want benchmark assets to remain distinct from holdings, so that benchmark setup does not contaminate portfolio state.
32. As a maintainer, I want Historical market data and Individual price separated into different Modules, so that each Module has stronger Locality.
33. As a maintainer, I want domain-named Modules, so that code navigation follows CONTEXT vocabulary.
34. As a maintainer, I want stateless service functions rather than a prepared object, so that the design matches project style and user preference.
35. As a maintainer, I want required FX inferred from assets, so that callers do not repeat Base currency rules.
36. As a maintainer, I want provider-specific FX pair strings hidden inside Implementation, so that callers use domain concepts.
37. As a maintainer, I want Historical market data valuation functions to fail clearly for missing required data, so that misuse is detected at the seam.
38. As a maintainer, I want Market data limitation values to be actionable-only, so that callers do not need to filter non-actionable cases.
39. As a maintainer, I want source-versus-cache distinction hidden from public Market data limitation values, so that fetch mechanics stay in Implementation and logs.
40. As a maintainer, I want Acceptable Morningstar lag removed from the public limitation classification, so that public types match domain meaning.
41. As a maintainer, I want warning string formatting outside market-data Modules, so that presentation text is not mixed with market-data rules.
42. As a maintainer, I want benchmark asset lookup outside Historical market data, so that Historical market data prepares supplied subjects rather than owning analytics setup.
43. As a maintainer, I want repository Modules to remain pure data access, so that business logic remains in service Modules.
44. As a maintainer, I want tests to use service-function Interfaces, so that the Interface is the test surface.
45. As a maintainer, I want only targeted persistence assertions for Forward-filled market data, so that tests do not overfit to Implementation details.
46. As a maintainer, I want rolling correlation left alone in this refactor, so that direct source fetching can be addressed in a later analytics deepening.
47. As an implementation agent, I want shallow helper exposure reduced, so that callers do not need to know ordering, FX identity, or fallback mechanics.
48. As an implementation agent, I want the architecture docs to reflect the new Module split, so that future agents do not rediscover stale design information.

## Implementation Decisions

- Build or modify a deep Historical market data Module.
- Build or modify a separate Individual price Module.
- Use domain names for the Modules rather than technical names.
- Expose stateless service functions, not a prepared object that carries database-reading behaviour.
- Use `HistoricalMarketDataPreparation` as the shared preparation result name.
- Historical market data preparation returns the Effective valuation date and actionable Market data limitation values.
- Historical market data infers required FX from supplied assets.
- Historical market data hides FX pair identity from callers.
- Historical market data owns asset price lookup identity: ticker for stocks and Morningstar code for funds/ETFs.
- Historical market data treats required fund/ETF assets without Morningstar codes as hard failures.
- Historical market data ignores same-day source values.
- Historical market data preserves persisted Forward-filled market data.
- Historical market data never forward-fills beyond the last source observation.
- Historical market data calculates the Effective valuation date as the minimum supported date across requested date, required asset prices, and required FX rates.
- Historical market data valuation read functions fail clearly when required data is absent.
- NAV rebuild fails on missing required valuation data instead of skipping the asset.
- Fetch failure with cached Historical market data remains usable.
- Fetch failure with no cached required Historical market data fails clearly.
- Market data limitation values are actionable-only.
- Source-versus-cache is not part of the public Market data limitation Interface.
- `Acceptable Morningstar lag` remains internal policy and does not create a returned Market data limitation.
- The public limitation classification should not include a non-actionable acceptable-lag variant.
- FX limitation subjects expose non-Base currency rather than provider-specific pair strings.
- Stock and FX stale policy continues to use Completed weekday cadence.
- The stale threshold remains an internal constant, not caller configuration.
- Individual price owns Live quote use, cached lookup, snapshot fallback, and display Market data limitation values.
- Individual price preserves softer fallback behaviour for portfolio display.
- Benchmark analytics owns benchmark asset lookup or creation outside Historical market data.
- NAV preparation Market data limitation values are not persisted in this PRD.
- User-facing warning formatting moves to presentation code.
- No schema changes are planned.
- Repository Modules remain data access only.
- Direct rolling correlation fetching remains out of scope.

## Testing Decisions

- Good tests should exercise external behaviour through Historical market data, Individual price, NAV rebuild, and presentation Interfaces rather than private helpers.
- Historical market data should be tested for Effective valuation date calculation across multiple assets and FX rates.
- Historical market data should be tested for FX inference from non-Base currency assets.
- Historical market data should be tested for implicit Base currency FX rate 1.0.
- Historical market data should be tested for missing required asset data failing clearly.
- Historical market data should be tested for missing required FX data failing clearly.
- Historical market data should be tested for missing Morningstar code on required fund/ETF assets.
- Historical market data should be tested for same-day source values being ignored.
- Historical market data should be tested for fetch failure with cached data moving the Effective valuation date earlier.
- Historical market data should be tested for fetch failure without cached required data failing clearly.
- Historical market data should be tested for persisted Forward-filled market data between source observations.
- Historical market data should be tested for no Forward-filled market data beyond the last source observation.
- Historical market data should be tested for Acceptable Morningstar lag limiting NAV without returning a Market data limitation.
- Historical market data should be tested for excessive Morningstar lag returning a Market data limitation.
- Historical market data should be tested for stock and FX stale policy using Completed weekday cadence.
- Historical market data should be tested for FX limitation subjects using non-Base currency.
- NAV tests should cover missing required valuation data failing before partial snapshots are written.
- Benchmark analytics tests should cover benchmark assets using Historical market data preparation while benchmark asset setup stays outside that Module.
- Individual price tests should cover Live quote use for stock display.
- Individual price tests should cover Live quote use for FX display.
- Individual price tests should cover funds/ETFs not using Live quote values.
- Individual price tests should cover snapshot fallback preserving display continuity.
- Individual price tests should cover Live quote stock display with stale cached FX returning an actionable Market data limitation.
- Presentation tests should cover warning text after formatting moves out of market-data logic.
- Existing market-data tests are prior art for preparation, same-day exclusion, forward-fill persistence, stale data, benchmark handling, and Individual price behaviour.
- Existing NAV tests are prior art for rebuild behaviour and snapshot correctness.
- Tests should use in-memory SQLite, dummy tickers, and the existing mock price fetcher.
- Tests must not make network calls.

## Out of Scope

- The already-implemented older Historical market data PRD is out of scope except as prior art.
- Transaction holding semantics are out of scope.
- Broader NAV daily rebuild orchestration is out of scope except for failing clearly on missing required valuation data.
- Portfolio position valuation deepening is out of scope except where it consumes Individual price functions and warning formatting.
- Price-fetching seam separation from stock fundamentals is out of scope.
- Fund look-through composition is out of scope.
- Period performance metrics are out of scope.
- Direct rolling correlation source fetching is out of scope.
- Configurable Base currency is out of scope.
- Exchange-specific market calendars are out of scope.
- Market-close-aware same-day Historical market data persistence is out of scope.
- Persisting NAV preparation Market data limitation values is out of scope.
- Database schema changes are out of scope.

## Further Notes

- This is a follow-up architectural deepening issue after the older Historical market data PRD implementation.
- The primary goal is Depth: callers should get more Leverage from smaller service-function Interfaces.
- The primary maintainer benefit is Locality: Historical market data rules and Individual price rules should stop changing in the same Module.
- The user explicitly prefers service functions over a prepared object that reads the database.
- The user explicitly does not need source-versus-cache distinction in public Market data limitation values.
- The user explicitly wants Acceptable Morningstar lag to be internal only, not a returned Market data limitation.
- No ADR conflict was found because no ADR files exist for this area.
