# Add Fund Quote Metadata To Single-Fund Analysis

Status: done

## Parent

`.scratch/fund-analysis-comparison/PRD.md`

## What to build

Add **Fund quote metadata** to single-fund analysis by fetching Morningstar quote metadata through the market data Module and displaying currency, AUM, inception date, and Morningstar fund-name fallback in `analyze fund`. Quote metadata must be a non-fatal enhancement: holdings and price history remain the core report inputs, while missing quote metadata shows `N/A` and logs a warning.

## Acceptance criteria

- [ ] `analyze fund --code <code>` continues to work for untracked **Fund candidate** values.
- [ ] Morningstar quote metadata is fetched through the market data Module, not directly from fund analysis.
- [ ] The Morningstar quote endpoint uses a dedicated quote URL setting and keeps required query params private to the source adapter.
- [ ] Fund quote metadata includes fund name, AUM, AUM currency, inception date, and quote currency where available.
- [ ] Local tracked-asset name wins over Morningstar fund name; Morningstar fund name is used when no local name exists; existing fallback remains for missing names.
- [ ] Displayed currency uses quote currency when available and falls back to existing holdings currency.
- [ ] Single-fund header shows `Currency | AUM | Inception | Total Holdings | Top 10 Weight | Portfolio Date` in that order.
- [ ] AUM uses existing European-style number formatting and includes a currency label.
- [ ] Inception date displays as `DD-MM-YYYY` after internal normalization.
- [ ] AUM and inception are always present in the header, using `N/A` when unavailable.
- [ ] Quote metadata failures do not fail the whole fund analysis; holdings or fund price history failures still fail as before.
- [ ] Quote metadata is not persisted in SQLite and is not included in holdings snapshot fingerprints.

## Blocked by

None - can start immediately
