# Add cache-first correlation market data Interface

Status: ready-for-agent

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Add a cache-first correlation market data Interface that prepares **Base currency** price series for Tracked assets and returns benchmark market data separately from Tracked asset series. Route correlation matrix analysis through this Interface while preserving aligned available series behaviour and benchmark-not-a-holding semantics.

## Acceptance criteria

- [ ] Correlation matrix analysis receives `&MarketData` and requests **Base currency** series from the market data Module instead of composing native prices and FX in the caller.
- [ ] The market data Module owns benchmark lookup or creation for correlation market data.
- [ ] Benchmark series is returned separately from Tracked asset series.
- [ ] Correlation market data goes cache-first and does not expose a direct-fetch public path.
- [ ] Correlation analysis uses aligned available **Base currency** series and does not force one **Effective valuation date**.
- [ ] Tests cover Tracked asset series, benchmark series, shared FX preparation, and benchmark separation through the public Interface.

## Blocked by

None - can start immediately
