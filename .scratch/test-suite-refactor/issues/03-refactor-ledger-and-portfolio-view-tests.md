Status: needs-triage

# Refactor Ledger and Portfolio View Tests

## Parent

.scratch/test-suite-refactor/PRD.md

## What to build

Refactor **Transaction ledger** and portfolio view tests into domain behavior buckets. The completed slice should make ledger workflows and current portfolio view behavior easy to find, while preserving coverage for import/export, edit/delete, validation, invalidation, current positions, returns, **Individual price**, acceptable Morningstar lag, and **Market data limitation** warning behavior.

## Acceptance criteria

- [ ] Buy, sell, dividend, split, import/export, edit/delete, validation, and invalidation tests are moved or reshaped into the ledger behavior bucket.
- [ ] Current position, return, **Individual price**, acceptable Morningstar lag, stale stock, stale FX, and **Market data limitation** warning tests are moved or reshaped into the portfolio view behavior bucket.
- [ ] Normal ledger workflows use production services by default so validation and invalidation behavior are exercised.
- [ ] Tests use behavior-style names without a redundant test prefix and use domain glossary terms where they clarify intent.
- [ ] Duplicate or low-signal ledger and portfolio view tests are merged or removed only when equivalent behavior remains covered by clearer scenarios.
- [ ] The migrated tests use the new harness, canonical fake data, deterministic dates, and domain-specific assertion helpers.

## Blocked by

- .scratch/test-suite-refactor/issues/01-establish-domain-test-harness.md
- .scratch/test-suite-refactor/issues/02-refactor-valuation-and-market-data-tests.md
