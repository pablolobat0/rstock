# Require Classified Tracked Assets Across Manual And CSV Workflows

Status: done

## Parent

.scratch/portfolio-first-cli-refactor/PRD.md

## What to build

Require Tracked assets to enter rstock with valid Asset classification from both manual creation and transaction CSV import. Extend the transaction CSV contract so export/import can round-trip classification and fund/ETF Morningstar codes, while preserving the current `Quantity` and `Price` column names and type-dependent meanings.

## Acceptance criteria

- [ ] Manual Tracked asset creation requires Asset classification.
- [ ] Asset classification validation rejects equity-specific attributes for incompatible top-level asset classes.
- [ ] Asset classification validation rejects fixed-income-specific attributes for incompatible top-level asset classes.
- [ ] Fund and ETF Tracked assets require Morningstar code at creation/import time.
- [ ] Stock Tracked assets do not require Morningstar code.
- [ ] Transaction CSV export writes classification fields and Morningstar code fields needed for round-trip import.
- [ ] Transaction CSV import requires the new schema and rejects the old 9-column schema clearly.
- [ ] Transaction CSV import rejects new Tracked assets with missing or inconsistent Asset classification.
- [ ] Transaction CSV import rejects fund/ETF Tracked assets without Morningstar code.
- [ ] Transaction CSV import preserves the current `Quantity` and `Price` meanings for buy, sell, dividend, and split rows.
- [ ] Tests cover the classification validation seam and CSV import/export round-trip behavior using in-memory SQLite and dummy tickers.

## Blocked by

None - can start immediately
