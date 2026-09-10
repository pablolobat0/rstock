# 04 — Persist the contiguous calculable prefix

## Parent

#62

## What to build

Make NAV preparation determine the first calendar date blocked by a required closing price or transaction-date FX rate. Persist every Complete NAV snapshot before that blocker, report the existing NAV-scoped Market data limitations, and make the preceding date the Effective valuation date. Do not reject an earlier calculable prefix because a later transaction or holding cannot yet be valued, and do not skip the blocked state transition to calculate later dates.

## Acceptance criteria

- [ ] The Effective valuation date is the final date in the contiguous calculable prefix after the NAV rebuild checkpoint.
- [ ] Closing-price requirements follow Positive-holding intervals rather than every asset across one common broad range.
- [ ] Buy, sell, and dividend transactions require the latest usable FX rate on or before their transaction date independently from end-of-day closing-price requirements.
- [ ] EUR transactions use the implicit Base currency FX rate of 1.0, and split transactions do not create an FX requirement.
- [ ] A later missing closing price persists every earlier Complete NAV snapshot and stops immediately before the first affected date.
- [ ] A later missing transaction-date FX rate for a buy, sell, or dividend persists every earlier Complete NAV snapshot and stops immediately before the transaction.
- [ ] Market data available after a blocked date does not allow NAV history to skip that date and continue from unknown state.
- [ ] When the first requested date is blocked, no new or seed snapshot is written and the existing checkpoint remains authoritative.
- [ ] Readiness returns the existing structured Asset or FX Market data limitations that explain the blocker.
- [ ] A full sale date requires transaction FX when applicable but requires no closing price when its final end-of-day quantity is zero.
- [ ] Retrying after the missing input becomes available resumes from the checkpoint and fills the formerly blocked range without changing earlier snapshots.
- [ ] Existing stale-data behavior still moves the Effective valuation date earlier without using Live quotes or data beyond the source-supported end.
- [ ] Tests exercise blockers introduced by later buys, sells, dividends, and held-asset valuations through the public readiness seam.
- [ ] Every persisted date remains a Complete NAV snapshot and all existing fully calculable scenarios retain equivalent values.
- [ ] Formatting, linting with warnings denied, and the complete test suite pass.

## Blocked by

- #65
