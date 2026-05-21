# Create market data directory Module for valuation

Status: ready-for-agent

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Route NAV valuation through the stateful market data Module public Interface. The slice should expose valuation preparation and strict valuation reads through the Module root, rename the NAV preparation result to valuation language, and preserve existing **Historical market data**, **Forward-filled market data**, **Effective valuation date**, **Base currency**, and **Market data limitation** behaviour.

## Acceptance criteria

- [ ] NAV receives `&MarketData` rather than a fetcher-shaped dependency.
- [ ] Callers use the market data Module root for valuation preparation and strict valuation reads.
- [ ] The NAV preparation result is renamed to valuation language and still returns the **Effective valuation date** plus **Market data limitation** values.
- [ ] Valuation preparation remains explicit and may write cached **Historical market data**; strict valuation reads do not write.
- [ ] NAV continues to fail clearly when required asset or FX market data is unavailable.
- [ ] Existing **Forward-filled market data**, **Acceptable Morningstar lag**, and **Completed weekday** stale-data behaviour is preserved.
- [ ] Tests cover the public valuation Interface and NAV caller path without relying on private helper functions.

## Blocked by

- .scratch/deepen-market-data-module-interface/issues/00-introduce-stateful-market-data-and-sources.md
