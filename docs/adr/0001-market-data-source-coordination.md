# ADR-0001: Market Data Owns Source Coordination

## Status

Accepted

## Context

rstock uses market data from multiple **Market data source** origins. Yahoo Finance supplies stock prices, FX rates, and stock info. Morningstar supplies fund and ETF price history, holdings, and fund metadata used for **Portfolio-relevant analysis**.

Historically, callers depended on a `PriceFetcher` Interface and some Morningstar paths bypassed that Interface through Python scripts. That made the market data area shallow in important places: callers and tests had to know too much about source lookup identity, FX pairs, subprocess contracts, token handling, and which path supplied which data.

The market data Module is being deepened so it owns coordination between source observations, cache repositories, **Forward-filled market data**, **Effective valuation date**, **Market data limitation**, and Base currency conversion. NAV remains the owner of NAV unitization logic.

## Decision

The market data Module will own source coordination and cache policy. It will receive source observations through an injected `MarketDataSources` Interface defined inside the market data Module.

`DefaultMarketDataSources` will be the public production source bundle. It will privately own Yahoo Finance and Morningstar source **Adapters**. Other Modules must not import or call Yahoo Finance or Morningstar **Adapters** directly.

`MarketDataSources` returns raw source observations only. Cache writes, **Forward-filled market data**, **Effective valuation date**, **Market data limitation**, and Base currency conversion remain inside `MarketData`.

FX source requests and FX cache persistence use source-neutral currencies rather than provider-specific pair strings. `MarketDataSources::exchange_rate_history` receives normalized `from` and `to` currencies. The FX cache stores `from_currency`, `to_currency`, `date`, and `rate`, with uniqueness on `(from_currency, to_currency, date)`. Provider-specific formatting such as Yahoo's FX ticker shape remains inside the source **Adapter**.

Production wiring constructs `DefaultMarketDataSources`, injects it into `MarketData`, then injects `MarketData` into NAV, portfolio, analytics, composition, and fund analysis Modules.

Tests may inject fake implementations of `MarketDataSources` into `MarketData` so tests can use mock source observations without network calls.

The Python Morningstar scripts will be replaced by Rust source **Adapter** implementation inside the market data Module. Current Morningstar endpoints are preserved for now. Morningstar token caching is private implementation detail using `~/.rstock/cache/morningstar_token.json`.

## Consequences

Source-specific behaviour has Locality inside market data. Replacing Morningstar or changing Yahoo Finance construction should not affect NAV, portfolio, analytics, composition, or fund analysis callers.

Provider-specific FX pair formatting no longer leaks into cache persistence or repository Interfaces.

The market data Interface has more Depth: callers ask for domain outcomes instead of coordinating source observations, cache state, FX conversion, and stale-data policy themselves.

`MarketDataSources` is a real Seam because there are production and test **Adapters**. The source **Adapters** behind `DefaultMarketDataSources` remain private because other Modules do not need them.

This introduces one boxed async trait dependency inside `MarketData`. The runtime cost is acceptable for a CLI and avoids spreading generics through service Modules.

## Alternatives Considered

- Keep `PriceFetcher`: rejected because it is too narrow for FX, stock info, fund holdings, and fund metadata.
- Expose Yahoo Finance and Morningstar **Adapters** publicly: rejected because it would leak source-specific details outside market data.
- Keep source fetching outside market data and inject it into callers: rejected because it weakens Locality and makes market data less Deep.
- Use feature-gated test support: rejected because integration tests should inject fake source observations without feature-flag ceremony.
- Use a concrete fake enum instead of a source Interface: rejected because the user preferred explicit source injection into `MarketData`.
