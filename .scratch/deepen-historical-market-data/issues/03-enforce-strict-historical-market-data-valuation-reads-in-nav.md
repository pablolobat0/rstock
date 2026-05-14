# Enforce Strict Historical Market Data Valuation Reads in NAV

Status: needs-triage

Type: AFK

## Parent

`.scratch/deepen-historical-market-data/PRD.md`

## What to build

Route NAV rebuild through strict Historical market data valuation service functions so required asset and FX valuation data must exist. NAV should fail clearly instead of silently skipping a held asset or missing required FX rate.

## Acceptance criteria

- [ ] Historical market data exposes service-function reads for required asset valuation in Base currency.
- [ ] Required asset valuation reads fail clearly when Historical market data is absent.
- [ ] Required FX valuation reads fail clearly when Historical market data is absent.
- [ ] NAV rebuild uses the strict valuation reads.
- [ ] NAV rebuild does not write partial snapshots when required asset valuation data is missing.
- [ ] NAV rebuild does not write partial snapshots when required FX valuation data is missing.
- [ ] NAV continues to ignore Live quote values and use Historical market data only.

## Blocked by

- `.scratch/deepen-historical-market-data/issues/02-infer-fx-inside-historical-market-data-preparation.md`
