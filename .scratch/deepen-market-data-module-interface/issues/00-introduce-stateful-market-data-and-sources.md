# Introduce stateful MarketData and source Interface

Status: done

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Introduce a stateful market data Module value and the injected `MarketDataSources` source Interface. This slice should establish the dependency shape before moving valuation, **Individual price**, analytics, or Morningstar implementation behind it.

## Acceptance criteria

- [ ] `src/services/market_data/` contains a Module root exposing `MarketData`.
- [ ] `MarketData::new(...)` accepts an injected `Box<dyn MarketDataSources>`.
- [ ] `MarketDataSources` lives under the market data Module and returns raw source observations only.
- [ ] `MarketDataSources` uses `NaiveDate` date inputs and returns `SourceObservation { date, value }` numeric series.
- [ ] `MarketDataSources::exchange_rate_history` accepts `from` and `to` currencies rather than provider-specific pair strings.
- [ ] `MarketData` normalizes currencies to uppercase before source and repository calls; repositories assume normalized input.
- [ ] `MarketData` validates FX currencies as three-letter alphabetic codes before source and repository calls.
- [ ] `DefaultMarketDataSources` is the public production source bundle.
- [ ] Concrete Yahoo Finance and Morningstar source **Adapters** are private implementation details under the market data Module.
- [ ] `main.rs` constructs `DefaultMarketDataSources`, injects it into `MarketData`, and passes `&MarketData` to existing callers.
- [ ] `tests/common` provides fake `MarketDataSources` for integration tests without network calls.
- [ ] Existing behaviour is preserved while the old `PriceFetcher` implementation can still be delegated to internally during this slice if needed.

## Blocked by

None - can start immediately
