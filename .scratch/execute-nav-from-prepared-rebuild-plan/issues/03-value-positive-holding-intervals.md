# 03 — Value only Positive-holding intervals

## Parent

#62

## What to build

Derive Positive-holding intervals from the checkpoint and ordered Transaction ledger, then prepare closing-price series only for dates when each performance asset has a positive end-of-day quantity. Closed assets must stop constraining later NAV dates, while a later repurchase starts a new interval. MarketData must continue to own Historical market data preparation, predecessor seeds, forward-fill policy, and valuation-series construction.

## Acceptance criteria

- [ ] Positive-holding intervals are derived using date and ascending transaction-ID order and the existing floating-point holding threshold.
- [ ] Buys can open an interval, a full sale closes it, and a later buy opens a separate interval.
- [ ] A date whose final end-of-day quantity is zero does not require a closing price for that asset.
- [ ] Same-day buys, sells, splits, and dividends preserve Transaction ledger ordering when determining the end-of-day holding requirement.
- [ ] Monetary holdings never create Positive-holding intervals or closing-price requirements for NAV.
- [ ] MarketData receives the required valuation ranges without exposing source adapters or moving cache and forward-fill policy into NAV.
- [ ] A range beginning after an earlier observation includes the predecessor seed needed for existing at-or-before and Forward-filled market data behavior.
- [ ] A predecessor seed never permits forward-fill beyond the final source-supported observation.
- [ ] A fully sold asset with no later prices does not shorten the Effective valuation date of assets that remain held and fully valued.
- [ ] A held, sold, and repurchased asset is valued during both intervals and not during the closed gap.
- [ ] Existing complete calendar-date history, unitization, and per-asset snapshot values remain equivalent when all required data is available.
- [ ] Tests use the public NAV readiness seam with fixed clocks and fake sources; private interval representations are not directly tested.
- [ ] Formatting, linting with warnings denied, and the complete test suite pass.

## Blocked by

- #63
