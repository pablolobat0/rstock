# ADR-0003: Compose the Portfolio View Outcome

## Status

Accepted

## Context

The Portfolio view currently uses a wide `PortfolioResult` that duplicates every `CurrentPositions` field, requires repeated manual copying, and uses nullable values for distinct meanings such as no applicable performance, missing market data, insufficient history, and mathematically undefined metrics. It also places independently dated current inventory beside synchronized NAV facts without making their different valuation scopes structural.

## Decision

The portfolio Module will expose `PortfolioView` as a composition of `PortfolioInventory` and a Portfolio performance state. The reusable Portfolio inventory projection will live in its own Module with a focused public Interface because both the Portfolio view and composition analysis consume it; Portfolio performance assembly remains private to the Portfolio view Module and reuses the existing NAV Interface.

Portfolio inventory contains separate performance-holding and Monetary-holding sections. Each section owns its positions, complete-or-unavailable aggregates, Daily-priced holdings movement, and current-inventory Market data limitations. The combined mixed-date Total value belongs to Portfolio inventory and remains distinct from synchronized performance value.

Portfolio performance is explicitly available, unavailable, or not applicable. Available performance requires an Effective valuation date, synchronized performance-holdings value, NAV, inception date, YTD, 1Y, 3Y, 5Y, and All period outcomes, and requested ready NAV history. All means since inception. Each period groups its return and risk facts, and calculated financial facts express available, unavailable, or not-applicable states independently. Market data limitations remain independent from fact availability and are scoped separately to current performance inventory, Monetary inventory, NAV/history readiness, and benchmark-dependent risk metrics.

Availability represents expected domain absence only. Database failures, malformed persisted data, violated ledger invariants, and missing required asset lookup metadata remain errors and do not produce a partial `PortfolioView`.

The Portfolio view Module will present one Interface that accepts the database, `MarketData`, and a presentation-neutral history request. Its Implementation hides readiness order, preparation reuse, concurrency, and history queries. The outcome remains derived from the Transaction ledger, market data, and existing NAV history; it is not persisted.

## Consequences

Composition analysis continues to call Portfolio inventory directly and cannot trigger NAV preparation. There is no public one-caller Portfolio performance pass-through Interface. `PortfolioResult` and `CurrentPositions` are removed rather than retained as compatibility aliases.

Tests use the same Interfaces as callers. The required domain matrix covers empty and Monetary-only inventory, blocked initial NAV, available but date-limited NAV, short period history, All as the since-inception period with the same return and risk facts, missing benchmark data with portfolio-only metrics still available, requested history, and Daily-priced holdings movement on weekends, weekdays without a new observation, partial coverage, current-day buys, FX movement, and separate performance and Monetary sections. No database migration is required.

## Alternatives Considered

- Keep one flattened result: rejected because it preserves duplicated fields, manual copying, and implicit valuation scopes.
- Compose inventory with optional performance: rejected because absence would still conflate not applicable and unavailable.
- Expose a separate public Portfolio performance operation: rejected because it currently has only one caller and would create a shallow hypothetical Seam.
- Persist the composed outcome: rejected because it would duplicate existing sources of truth and introduce new invalidation rules.
