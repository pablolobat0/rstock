# ADR-0003: One Authoritative Transaction Ledger Projection

## Status

Accepted

## Context

Quantity, split, cost, dividend, cash-flow, and open-holdings rules are currently repeated across the transaction model and the portfolio, NAV, analytics, import, and transaction services. Repository ordering alone cannot protect in-memory imports or hypothetical edits and deletes, and validating only the changed entry can leave a later ledger prefix invalid.

## Decision

`src/services/ledger.rs` will own one pure Transaction ledger replay engine. An opaque canonical ledger constructor will sort persisted entries by `(date, id)`, verify identity integrity, and make ordering a ledger invariant rather than a repository responsibility. Repository `ORDER BY` clauses remain useful query optimizations. An edited entry keeps its ID and is ordered by `(new_date, existing_id)`; inserted entries use their database-generated IDs, and same-date imported entries preserve insertion order through ascending generated IDs.

Replay will validate every chronological prefix and emit semantic transitions rather than only final holdings or validated raw rows. Transitions expose quantity before and after each entry and typed buy contribution, sell withdrawal and cost-removal, dividend income, and split effects. Portfolio, NAV, analytics, import, and transaction mutation paths must consume these effects rather than reinterpret persisted columns. Quantity arithmetic uses `FLOAT_EPSILON`: values within positive or negative epsilon are normalized to zero, values below negative epsilon are invalid, and open-holding preconditions require quantity above epsilon.

All buy, sell, dividend, split, edit, delete, and CSV import paths first validate the mutation's field shapes, then begin one database transaction and tentatively apply the mutation. They reload every affected asset's exact persisted ledger, canonicalize and replay it, invalidate split-price caches and Complete NAV snapshots only after successful replay, and commit. Any shape, persistence, replay, or invalidation failure rolls the transaction back, so an invalid ledger is never externally visible or durable. Mutation validation starts from zero and includes every later entry. A dividend requires positive open quantity at its exact ledger position. Replay stops at the first invalid canonical prefix and reports the asset, persisted transaction or import source identity, date, entry type, quantity before the entry, attempted effect, and violated invariant; it does not return a partial projection.

Market data preparation remains outside the ledger engine. Every monetary component of an entry is denominated in its Tracked asset's native currency. `MarketData` prepares Historical market data, then a pure ledger enrichment helper applies an in-memory latest-rate-on-or-before lookup to transaction-date monetary effects. Missing FX makes only dependent Base currency facts unavailable. NAV consumes grouped daily semantic effects, owns share issuance and redemption, valuation, dividend cash, and snapshot persistence, and excludes Monetary holdings according to Asset classification. Incremental NAV replay may seed quantities from the last Complete NAV snapshot; mutation validation may not use a seed.

The `transactions` table will retain one global ID and one chronology but replace overloaded values with type-specific nullable columns: `units` and `unit_price_cents` for buys and sells, `dividend_amount_cents` and `dividend_deductions_cents` for dividends, and `split_ratio` for splits. `fees_cents` applies only to buys and sells. Database shape constraints will reject irrelevant field combinations and invalid signs. A Gross dividend distribution is positive, its deductions satisfy `0 <= deductions <= gross`, and its projected Net dividend income cannot be negative. A persisted entry's Tracked asset and transaction type are immutable; ordinary edits may change only its date and fields meaningful to that existing type.

No atomic replacement, privileged identity edit, or automatic historical repair operation will be added. Correcting an old asset or transaction-type mistake requires removing dependent entries in an order that leaves each intermediate ledger valid, then rebuilding the history through normal commands.

The SQLite migration will preserve IDs, `(date, id)` chronology, monetary cents, and transaction meaning in both up and down directions, and will recreate the global and per-asset chronology indexes. Existing persisted ledgers are assumed valid; no compatibility, cleanup, grandfathering, or automatic-repair path will be added.

Pure deterministic replay and FX-enrichment tests are the executable specification. Focused integration tests will cover repository mapping, every hypothetical mutation path, atomic rollback and invalidation, same-day ordering, seeded-versus-full NAV equivalence, Monetary exclusion, and exact round-trip schema migration with constraint enforcement. Property-based testing is not introduced.

## Consequences

Transaction interpretation and validity have one testable source of truth without coupling the pure engine to SeaORM, async market data, NAV unitization, or Asset classification. Consumers retain their own projections but receive semantic effects instead of duplicating ledger arithmetic.

The migration is more work than hiding the existing encoding behind typed models, but it makes invalid transaction shapes unrepresentable at the persistence boundary while preserving externally meaningful transaction IDs and ordering.

## Alternatives Considered

- Repository-owned ordering was rejected because imports and hypothetical mutations also require canonical chronology.
- Repository-backed or async FX-aware replay was rejected because quantity validity must remain pure and independent of database and market-data availability.
- Final-state-only replay was rejected because NAV and prefix validation require chronological transitions.
- Separate transaction subtype tables were rejected because they complicate global identity and chronology in SQLite.
- Simulating pending mutations entirely in memory was rejected because synthetic sequence keys would need to predict generated-ID ordering. Tentative persistence inside the caller-owned transaction establishes exact chronology, while replay failure still rolls back atomically.
