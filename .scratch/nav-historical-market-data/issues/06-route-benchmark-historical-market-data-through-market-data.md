# Route Benchmark Historical Market Data Through Market Data

Status: done

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Move benchmark price and benchmark FX preparation onto the same historical availability workflow as holdings. Benchmark market data should follow historical market-data rules while remaining distinct from holdings and portfolio state.

## Acceptance criteria

- [x] Benchmark historical prices are prepared through the market-data Module.
- [x] Benchmark FX is prepared through the same historical availability workflow when the benchmark currency differs from the Base currency.
- [x] Benchmark market data is not treated as a holding.
- [x] Existing benchmark-dependent analysis continues to work with the market-data Module path.
- [x] Tests cover benchmark price preparation, benchmark FX preparation, and benchmark data staying distinct from holdings.

## Blocked by

None - can start immediately
