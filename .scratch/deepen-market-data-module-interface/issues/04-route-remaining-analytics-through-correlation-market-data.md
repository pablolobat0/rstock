# Route remaining analytics through correlation market data

Status: done

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Route rolling correlation and portfolio risk benchmark data through the cache-first market data Interface. Carry non-blocking **Market data limitation** values into analytics results and display them separately from insufficient-data warnings.

## Acceptance criteria

- [ ] Rolling correlation receives `&MarketData` and uses cache-first market data for both selected Tracked assets instead of direct fetching.
- [ ] Portfolio period risk metrics use benchmark data prepared by the market data Module.
- [ ] Analytics **Market data limitation** values are non-blocking when enough aligned data exists.
- [ ] Missing required series data remains a clear error where analytics cannot build a usable series.
- [ ] Insufficient aligned data remains an analytics warning, not a **Market data limitation**.
- [ ] Correlation matrix, rolling correlation, and portfolio output display threshold-qualified analytics **Market data limitation** values.
- [ ] Tests cover limitation propagation and display for benchmark and Tracked asset analytics paths.

## Blocked by

None - can start immediately
