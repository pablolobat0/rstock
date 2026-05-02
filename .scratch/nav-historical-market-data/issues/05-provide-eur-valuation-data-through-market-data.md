# Provide EUR Valuation Data Through Market Data

Status: ready-for-agent

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Have market data provide valuation-ready data for callers: native asset price, FX rate, and EUR valuation price where relevant. EUR assets should use the Base currency implicit FX rate of 1.0, and non-EUR assets should require explicit FX market data.

## Acceptance criteria

- [ ] Market data can provide native price, FX rate, and EUR valuation price for valuation callers.
- [ ] EUR assets use an implicit FX rate of 1.0.
- [ ] Non-EUR assets require explicit FX market data for EUR valuation.
- [ ] NAV valuation callers no longer need to reimplement base-currency branching.
- [ ] Tests cover EUR assets, non-EUR assets with FX data, and non-EUR assets missing required FX data.

## Blocked by

None - can start immediately
