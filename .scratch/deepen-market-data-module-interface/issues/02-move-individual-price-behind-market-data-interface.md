# Move Individual price behind market data Interface

Status: ready-for-agent

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Move display-time per-asset pricing behind the market data Module Interface using the domain term **Individual price**. The slice should add a fallback input value, route portfolio rows through the new Interface, and preserve **Live quote**, fund/ETF, snapshot fallback, **Base currency** conversion, and **Market data limitation** behaviour.

## Acceptance criteria

- [ ] Display-time market data output is renamed to **Individual price** terminology.
- [ ] The **Individual price** Interface accepts a fallback value instead of separate fallback native price, price date, and FX arguments.
- [ ] Portfolio row construction receives `&MarketData` and uses the market data Module root for **Individual price**.
- [ ] Stock **Live quote** behaviour, FX **Live quote** behaviour, fund/ETF non-live behaviour, and snapshot fallback are preserved.
- [ ] A stock **Live quote** combined with stale cached FX can still return a threshold-qualified **Market data limitation**.
- [ ] Tests cover the public **Individual price** Interface and portfolio display consumption.

## Blocked by

None - can start immediately
