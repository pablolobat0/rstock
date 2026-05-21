# Migrate FX cache to currency columns

Status: ready-for-agent

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Replace provider-style FX pair storage in the `daily_exchange_rates` cache with source-neutral currency columns. The cache and repository Interface should use `from_currency` and `to_currency`, while provider-specific formatting remains inside source **Adapters**. Existing cached FX rows do not need to be preserved.

## Acceptance criteria

- [ ] A migration drops and recreates `daily_exchange_rates` with `from_currency`, `to_currency`, `date`, and `rate`.
- [ ] The new table enforces uniqueness on `(from_currency, to_currency, date)`.
- [ ] Existing cached FX rows are not preserved; they can be refetched as **Historical market data**.
- [ ] The SeaORM entity for `daily_exchange_rates` uses `from_currency`, `to_currency`, `date`, and `rate`.
- [ ] `exchange_rate_repo` functions accept `from_currency` and `to_currency` instead of provider-style pair strings.
- [ ] Existing FX cache lookups, at-or-before lookups, range queries, existence checks, and upserts preserve behaviour.
- [ ] Tests and test helpers insert and assert exchange rates using `from_currency` and `to_currency`.
- [ ] Provider-specific FX formatting such as Yahoo ticker construction is not stored in the database and is not required by repository callers.

## Blocked by

None - can start immediately
