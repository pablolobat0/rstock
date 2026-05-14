# Resolve Price Lookup Identity Inside Market Data

Status: done

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Make the market-data Module own price lookup identity for historical prices. Stocks should use ticker. Funds and ETFs should use Morningstar code. A held fund or ETF without a Morningstar code should fail NAV market-data preparation with a clear reason.

## Acceptance criteria

- [x] Stock historical price lookup uses ticker through the market-data Module.
- [x] Fund and ETF historical price lookup uses Morningstar code through the market-data Module.
- [x] A held fund or ETF without a Morningstar code fails NAV market-data preparation clearly.
- [x] Callers no longer need to resolve price lookup identity before asking for historical market data.
- [x] Tests cover stock lookup, fund/ETF lookup, and missing Morningstar code for a held fund or ETF.

## Blocked by

None - can start immediately
