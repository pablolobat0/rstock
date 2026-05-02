# Enforce NAV Effective Valuation Date From Required Market Data

Status: done

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Introduce the deeper market-data Module path for NAV historical preparation. NAV should still decide which assets and FX pairs are required, but market data should prepare those requirements, calculate the effective valuation date, and prevent NAV from writing partial snapshots when any required asset or FX data is missing.

## Acceptance criteria

- [x] NAV uses one effective valuation date derived from all required asset prices and required FX rates.
- [x] NAV fails clearly when a held asset has no required historical market data.
- [x] NAV fails clearly when a required FX rate has no historical market data.
- [x] NAV does not write partial snapshots when required market data is missing.
- [x] Tests cover missing required asset data, missing required FX data, and the minimum effective valuation date across multiple requirements.

## Blocked by

None - can start immediately
