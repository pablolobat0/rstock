# Preserve Historical Market Data Cache Semantics

Status: ready-for-agent

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Move historical cache preparation and forward-filled market data rules into the market-data Module while preserving current cache semantics: completed-date persistence, no same-day historical caching, and no forward-fill beyond the last source observation.

## Acceptance criteria

- [ ] Historical market data is persisted only for completed dates before today.
- [ ] Same-day live quote values are not persisted as historical market data.
- [ ] Forward-filled market data is persisted between source observations.
- [ ] Forward-filled market data is never created beyond the last source observation.
- [ ] Existing daily price and FX cache behaviours remain covered by tests through the market-data Module Interface.

## Blocked by

None - can start immediately
