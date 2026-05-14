# Surface Market Data Limitations In User-Facing Output

Status: done

## Parent

.scratch/nav-historical-market-data/PRD.md

## What to build

Implement the user-facing presentation for market data limitations in portfolio output. Normal Acceptable Morningstar lag should not create noisy warnings. Actionable stale stock/FX data and excessive Morningstar lag should be visible to the user.

## Acceptance criteria

- [x] Portfolio output can surface actionable market data limitations.
- [x] Acceptable Morningstar lag of seven days or less is suppressed from user-facing warnings.
- [x] Morningstar lag greater than seven days is visible as a user-facing warning or limitation.
- [x] Stock and FX stale-data limitations based on completed weekday cadence are visible when actionable.
- [x] Tests cover warning visibility and suppression rules at the output level.

## Blocked by

None - can start immediately
