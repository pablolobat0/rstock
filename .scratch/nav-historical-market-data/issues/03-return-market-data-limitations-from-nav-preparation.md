# Return Market Data Limitations From NAV Preparation

Status: ready-for-agent

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Return structured market data limitations from NAV market-data preparation. Stale cached market data should remain usable and move the effective valuation date earlier. Missing market data should remain a hard failure. Normal Morningstar lag and completed-weekday stock/FX stale data should be classified so callers can decide what to show.

## Acceptance criteria

- [ ] Stale cached market data is returned as a market data limitation rather than only logged.
- [ ] Missing market data remains distinct from stale market data and still fails NAV preparation.
- [ ] Acceptable Morningstar lag of seven days or less is classified separately from actionable Morningstar lag.
- [ ] Stock and FX stale-data limitations use completed weekday cadence and ignore weekends.
- [ ] Tests cover stale cache fallback, Acceptable Morningstar lag, excessive Morningstar lag, and stock/FX completed-weekday stale data.

## Blocked by

- .scratch/nav-historical-market-data/issues/02-preserve-historical-market-data-cache-semantics.md
