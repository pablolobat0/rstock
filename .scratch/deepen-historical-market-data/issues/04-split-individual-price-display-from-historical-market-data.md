# Split Individual Price Display from Historical Market Data

Status: needs-triage

Type: AFK

## Parent

`.scratch/deepen-historical-market-data/PRD.md`

## What to build

Move Individual price display behaviour into a separate domain-named Module. Portfolio display should keep showing current useful values, while Live quote use, cached lookup, snapshot fallback, and display Market data limitation values are no longer mixed with Historical market data preparation.

## Acceptance criteria

- [ ] Individual price display logic lives behind an Individual price service-function Interface.
- [ ] Stock display can use non-persisted Live quote values.
- [ ] FX display can use non-persisted Live quote values.
- [ ] Funds and ETFs do not use Live quote values for display.
- [ ] Snapshot fallback preserves portfolio row rendering when current display data is unavailable.
- [ ] A stock Live quote combined with stale cached FX returns an actionable Market data limitation.
- [ ] Portfolio output remains behaviourally equivalent except for actionable-only limitation semantics from the parent PRD.
- [ ] Tests cover Live quote stock display, Live quote FX display, fund/ETF display, fallback, and stale FX limitation behaviour.

## Blocked by

- `.scratch/deepen-historical-market-data/issues/01-make-market-data-limitation-actionable-only.md`
