# Route Individual Price Display Through Market Data

Status: done

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Move individual price display data onto the market-data Module. Stocks and FX may use non-persisted live quotes for current display. Funds and ETFs on the Morningstar price path should not use live quotes. A live stock price may be combined with stale cached FX when live FX is unavailable, but the result must carry a market data limitation.

## Acceptance criteria

- [x] Individual stock display data can use non-persisted live quotes.
- [x] Individual FX display data can use non-persisted live quotes.
- [x] Live quote values are not persisted as historical market data.
- [x] Funds and ETFs on the Morningstar price path do not use live quotes for display.
- [x] Live stock data combined with stale cached FX returns a market data limitation.
- [x] Tests cover live stock display, live FX display, fund/ETF display without live quotes, and stale cached FX limitation.

## Blocked by

None - can start immediately
