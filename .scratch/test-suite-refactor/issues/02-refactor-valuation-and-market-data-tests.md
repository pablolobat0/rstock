Status: needs-triage

# Refactor Valuation and Market Data Tests

## Parent

.scratch/test-suite-refactor/PRD.md

## What to build

Refactor the valuation and market data parts of the test suite into domain behavior buckets. The completed slice should make **NAV**, portfolio history, dividends, splits, currency conversion, failure atomicity, **Effective valuation date**, **Historical market data**, FX, **Forward-filled market data**, **Stale market data**, **Live quote**, provider identity, and benchmark behavior easy to find and verify through behavior-style tests.

## Acceptance criteria

- [ ] NAV, portfolio history, dividend, split, currency conversion, failure atomicity, and **Effective valuation date** tests are moved or reshaped into the valuation behavior bucket.
- [ ] Price, FX, provider identity, **Forward-filled market data**, **Stale market data**, **Live quote**, benchmark, and no-network mock behavior tests are moved or reshaped into the market data behavior bucket.
- [ ] Tests use behavior-style names without a redundant test prefix and use domain glossary terms where they clarify intent.
- [ ] Duplicate or low-signal valuation and market data tests are merged or removed only when equivalent behavior remains covered by clearer scenarios.
- [ ] Real-clock dependence is removed where feasible for the migrated valuation and market data tests.
- [ ] The migrated tests use the new harness, canonical fake data, deterministic dates, and domain-specific assertion helpers.

## Blocked by

- .scratch/test-suite-refactor/issues/01-establish-domain-test-harness.md
