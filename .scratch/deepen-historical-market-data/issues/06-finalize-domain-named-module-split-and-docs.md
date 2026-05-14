# Finalize Domain-Named Module Split and Docs

Status: needs-triage

Type: AFK

## Parent

`.scratch/deepen-historical-market-data/PRD.md`

## What to build

Complete the architecture deepening by finalizing the Historical market data and Individual price Module split, removing shallow compatibility helpers where practical, moving user-facing warning text into presentation code, and updating architecture documentation to match the new Module shape.

## Acceptance criteria

- [ ] Historical market data and Individual price are represented as domain-named Modules.
- [ ] Shallow public helpers for FX identity or mixed market-data behaviour are removed where no longer needed.
- [ ] User-facing Market data limitation warning text is formatted in presentation code rather than market-data Modules.
- [ ] Architecture documentation reflects Historical market data and Individual price as separate Modules.
- [ ] Repository Modules remain pure data access.
- [ ] Rolling correlation direct source fetching remains unchanged and out of scope.
- [ ] Tests or compile checks prove callers use the new Module paths.

## Blocked by

- `.scratch/deepen-historical-market-data/issues/03-enforce-strict-historical-market-data-valuation-reads-in-nav.md`
- `.scratch/deepen-historical-market-data/issues/04-split-individual-price-display-from-historical-market-data.md`
- `.scratch/deepen-historical-market-data/issues/05-route-benchmark-analytics-through-historical-market-data-as-supplied-asset.md`
