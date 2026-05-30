# Create Compare Funds Command With Info, Performance, Allocations, And Common Holdings

Status: ready-for-agent

## Parent

`.scratch/fund-analysis-comparison/PRD.md`

## What to build

Add the first complete `compare funds` vertical slice: a top-level `compare` command group with a `funds` subcommand that compares two Morningstar fund codes. The command should show fund identity, fund info, multi-period performance metrics, side-by-side allocation comparisons, and **Common fund holding** rows. It should allow untracked codes, reject identical codes, and avoid portfolio rebuild.

## Acceptance criteria

- [ ] `compare funds --code-a <code> --code-b <code> --period <period>` parses and dispatches successfully.
- [ ] `--period` accepts `30d`, `6m`, `1y`, `3y`, and `5y`, and defaults to `1y`.
- [ ] Identical fund codes are rejected with a clear validation error.
- [ ] Untracked Morningstar codes are allowed.
- [ ] Labels use local tracked-asset names when available, Morningstar names when no local name exists, and code fallback when no name exists.
- [ ] Tables use full fund names without codes; the top identity section includes codes for disambiguation.
- [ ] The command fetches holdings, quote metadata, and price history for both funds where needed, preferably concurrently.
- [ ] Holdings or price history failure for either fund fails the comparison; quote metadata failure is non-fatal and displays `N/A`.
- [ ] `compare funds` does not trigger a portfolio rebuild.
- [ ] Fund info comparison includes currency, AUM, inception date, total holdings, top 10 weight, and portfolio date.
- [ ] Performance comparison covers YTD, 1Y, 3Y, 5Y, and all time, with periods as columns and one row per metric per fund.
- [ ] Each unavailable performance metric cell shows `N/A` independently.
- [ ] Beta remains versus the configured benchmark.
- [ ] Sector, country, and currency allocation comparisons use the union of categories from both funds.
- [ ] Missing allocation category weights display as `0,00%`.
- [ ] Allocation comparison rows sort by the larger fund weight descending.
- [ ] Common holdings are computed from up to 200 holdings per fund.
- [ ] Common holdings match by ticker when available on both sides, otherwise by normalized name.
- [ ] Name normalization trims whitespace, ignores case, and collapses repeated spaces; fuzzy matching is not implemented.
- [ ] Common holdings include all reported holding types and sort by the larger fund weight descending.
- [ ] Common holdings use fixed columns for ticker, first fund holding, first fund weight, second fund holding, and second fund weight.
- [ ] Missing tickers display as `—`.
- [ ] Service tests cover common-holding matching by ticker, normalized name, non-matching similar names, and max-weight sorting.

## Blocked by

None - can start immediately
