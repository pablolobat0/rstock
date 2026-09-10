# 05 — Bound atomic snapshot persistence

## Parent

#62

## What to build

Bound generated NAV output by both calendar-date and total snapshot-row targets while preserving the Complete NAV snapshot as the indivisible persistence unit. Large portfolios must not accumulate an unbounded number of per-asset rows merely because the date target has not been reached, and interruption must still resume from the last committed checkpoint.

## Acceptance criteria

- [ ] Persistence flushes generated output when either the date target or total generated-row target would be exceeded.
- [ ] One portfolio row and every required per-asset row for its date always share one caller-owned database transaction.
- [ ] A Complete NAV snapshot is never split to satisfy the row target.
- [ ] One date that exceeds the row target by itself is permitted as an oversized atomic persistence unit.
- [ ] Seed snapshots, fully liquidated dates, and other valid zero-detail dates remain valid Complete NAV snapshots.
- [ ] Repository-level statement chunks remain inside the caller-owned transaction and do not independently commit.
- [ ] An injected late per-asset write failure rolls back every date in the active persistence transaction.
- [ ] Complete dates committed by earlier transactions remain intact after an interruption.
- [ ] A later readiness call resumes after the latest committed checkpoint and produces history equivalent to an uninterrupted rebuild.
- [ ] Public-seam tests exercise both date-triggered and row-triggered flushes without asserting private buffer types or helper boundaries.
- [ ] No staging table, completion marker, status column, or other schema migration is introduced.
- [ ] Formatting, linting with warnings denied, and the complete test suite pass.

## Blocked by

- #63
