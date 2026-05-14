# Remove Shallow Market Data Orchestration Paths

Status: done

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Clean up obsolete shallow market-data orchestration after NAV, benchmark, and individual display paths consume the deeper market-data Module. Repository Modules should remain pure data access, fetch adapters should remain thin, and portfolio asset history reuse should remain outside market data.

## Acceptance criteria

- [x] Obsolete duplicate market-data orchestration is removed or reduced after callers move to the deeper market-data Module.
- [x] Repository Modules remain data access Modules without market-data business rules.
- [x] Fetch adapters remain thin adapters at the price-fetching seam.
- [x] Portfolio asset history reuse remains outside the market-data Module.
- [x] Existing market-data, NAV, benchmark, and display tests continue to pass through the deeper Module path.

## Blocked by

None - can start immediately
