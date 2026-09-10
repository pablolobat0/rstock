# PRD: Execute NAV from a Prepared Rebuild Plan

Status: ready-for-agent

## Problem Statement

As a portfolio owner, I need NAV readiness and rebuilding to remain fast as my Transaction ledger and Complete NAV snapshot history grow. Routine readiness currently replays and compares all historical holdings and per-asset snapshots even when history is already current, so a warm portfolio command performs work proportional to the entire history instead of the new history it must calculate. Rebuild preparation also treats every asset touched by the range as requiring market data across the broad common range, which can let an asset sold long ago constrain later NAV dates unnecessarily.

The calculation and persistence paths have already removed much of the former per-day and per-holding SQL by preloading valuation series and batching writes, but the preparation is not yet expressed as one authoritative execution plan. The boundary between NAV-owned ledger state and MarketData-owned valuation policy is implicit, input ranges are broader than Positive-holding intervals, the output batch is bounded only by date count rather than generated row count, and successful preparation does not expose a testable contract that execution is independent of database reads.

The optimization must not weaken portfolio history. NAV history must retain a Complete NAV snapshot for every calculable calendar date, every portfolio row must remain atomic with all required per-asset rows, a later missing price or FX requirement must not discard an earlier calculable prefix, and interruption must resume from the last committed NAV rebuild checkpoint.

## Solution

Prepare one immutable, NAV-owned `NavRebuildPlan` for the complete rebuild range before executing NAV. The plan will own the latest NAV rebuild checkpoint state, ordered Transaction ledger entries, participating Tracked assets, Positive-holding intervals, and range-loaded valuation data produced by MarketData. Once preparation succeeds, plan execution will perform no database reads or fallback valuation lookups; missing in-memory data will be an invariant failure rather than an invitation to query during the daily loop.

MarketData will continue to own Historical market data preparation, source coordination, cache policy, forward-fill rules, Market data limitations, and valuation series. NAV will own holding-state replay, Positive-holding interval derivation, transaction effects, unitization, the contiguous calculable prefix, Complete NAV snapshot generation, and persistence orchestration. Closing prices will be required only when an asset has a positive end-of-day performance holding. Transaction-date FX will be required separately for buys, sells, and dividends.

The Effective valuation date will be the final date in the contiguous calculable prefix after the NAV rebuild checkpoint. If a required input first becomes unavailable on a later date, all earlier calculable dates will still be persisted and later dates will wait for a subsequent readiness attempt. Generated snapshots will be persisted in transactions bounded by date and row targets, but a Complete NAV snapshot will never be split. Routine readiness will trust the latest Complete NAV snapshot as authoritative and will stop reauditing all earlier history.

## User Stories

1. As a portfolio owner, I want warm NAV readiness to avoid scanning my complete historical detail, so that routine portfolio commands stay responsive as history grows.
2. As a portfolio owner, I want NAV history to contain every calculable calendar date, so that performance history remains complete rather than sparse or lazy.
3. As a portfolio owner, I want every persisted portfolio snapshot to include all required per-asset detail, so that each date remains auditable.
4. As a portfolio owner, I want portfolio and per-asset rows for a date committed atomically, so that interruption cannot leave a partial snapshot.
5. As a portfolio owner, I want an interrupted rebuild to retain previously committed complete dates, so that successful work is not discarded.
6. As a portfolio owner, I want a later readiness call to resume after the latest committed Complete NAV snapshot, so that interrupted work is not repeated from inception.
7. As a portfolio owner, I want the latest Complete NAV snapshot treated as the NAV rebuild checkpoint, so that routine readiness can extend trusted history directly.
8. As a portfolio owner, I want a later missing market-data requirement not to discard earlier calculable dates, so that history advances as far as current evidence permits.
9. As a portfolio owner, I want the Effective valuation date to be the end of the contiguous calculable prefix, so that NAV never skips an uncalculable state transition.
10. As a portfolio owner, I want a missing input on a future transaction date to stop history immediately before that transaction, so that later NAV is not calculated from unknown state.
11. As a portfolio owner, I want a sold asset to stop constraining later NAV dates, so that stale data for closed positions does not make current holdings appear older.
12. As a portfolio owner, I want an asset closing price required only while its end-of-day quantity is positive, so that valuation requirements match actual holdings.
13. As a portfolio owner, I want a full sale date with zero end-of-day quantity not to require a closing price, so that the supplied transaction facts can close the position without an unnecessary valuation.
14. As a portfolio owner, I want a later repurchase to begin a new Positive-holding interval, so that closing-price requirements restart when the asset contributes to NAV again.
15. As a portfolio owner, I want same-day Transaction ledger entries applied in date and ascending ID order, so that Positive-holding intervals and NAV state remain deterministic.
16. As a portfolio owner, I want buy, sell, and dividend conversion to require transaction-date FX independently from end-of-day valuation, so that Base currency unitization remains correct.
17. As a portfolio owner, I want EUR transactions and holdings to retain the implicit FX rate of 1.0, so that Base currency assets do not require external FX data.
18. As a portfolio owner, I want Monetary holdings excluded from NAV planning and valuation, so that performance measurement retains its existing scope.
19. As a portfolio owner, I want dividends to continue accumulating as cash in total portfolio value, so that the prepared plan preserves existing NAV economics.
20. As a portfolio owner, I want buys to issue NAV shares using the existing unitization rules, so that deposits do not create artificial performance.
21. As a portfolio owner, I want sells to redeem NAV shares using the existing unitization rules, so that withdrawals do not create artificial performance.
22. As a portfolio owner, I want splits to change holding quantity without changing portfolio economics, so that split history remains correct.
23. As a portfolio owner, I want a new portfolio to retain its initial NAV seed snapshot, so that inception performance remains anchored at 100.
24. As a portfolio owner, I want a Positive-holding interval beginning on a weekend or holiday to use the latest earlier Historical market data seed, so that calendar-date NAV follows existing forward-fill rules.
25. As a portfolio owner, I want forward-filled market data never to extend beyond the source-supported end, so that the plan does not invent later valuations.
26. As a portfolio owner, I want missing or stale data represented through existing Market data limitations, so that readiness communicates why history stopped.
27. As a portfolio owner, I want current normalized portfolio and per-asset history retained, so that aggregate NAV reads stay efficient and historical composition remains auditable.
28. As a portfolio owner, I want a fully liquidated date to remain a valid Complete NAV snapshot with no per-asset rows, so that completeness does not falsely require a holding.
29. As a portfolio owner, I want a seed date to remain a valid Complete NAV snapshot with no per-asset rows, so that initialization is not mistaken for corruption.
30. As a maintainer, I want one immutable plan to contain every input required by NAV execution, so that the execution phase has a clear and enforceable boundary.
31. As a maintainer, I want NAV to own the rebuild plan, so that unitization, ledger replay, holding intervals, and snapshot production remain local to NAV.
32. As a maintainer, I want MarketData to produce the plan's valuation-series portion, so that source and cache policy do not leak into NAV.
33. As a maintainer, I want successful plan preparation to guarantee SQL-free execution, so that per-day and per-holding reads cannot return through fallback code.
34. As a maintainer, I want missing in-memory valuation data treated as an invariant failure, so that an incomplete plan is detected rather than hidden by repository access.
35. As a maintainer, I want all rebuild transactions loaded once in ledger order, so that the executor can replay them without repeated reads.
36. As a maintainer, I want required assets loaded once for checkpoint holdings and rebuild transactions, so that asset metadata is not repeatedly queried.
37. As a maintainer, I want prices and FX loaded as range series with predecessor seeds, so that lookups remain in memory while preserving at-or-before semantics.
38. As a maintainer, I want preparation work not to grow with rebuilt calendar dates or holding-days, so that query count reflects data subjects rather than the daily calculation loop.
39. As a maintainer, I want output memory bounded by both calendar-date and generated-row targets, so that portfolios with many simultaneous holdings do not create unexpectedly large batches.
40. As a maintainer, I want one oversized date allowed to exceed the row target, so that memory tuning never weakens Complete NAV snapshot atomicity.
41. As a maintainer, I want repository bulk-write chunks to remain inside the caller-owned persistence transaction, so that internal statement limits do not create partial commits.
42. As a maintainer, I want routine readiness to trust atomic persistence rather than replay all history, so that warm work is proportional to missing history.
43. As a maintainer, I want the old automatic repair expectations removed from routine readiness tests, so that tests match the authoritative checkpoint policy.
44. As a maintainer, I want one public NAV readiness interface to remain the behavioral test seam, so that private planner and executor structure can evolve safely.
45. As a maintainer, I want performance evidence collected through the existing Criterion NAV paths, so that the optimization is compared with established fixtures and baseline procedures.
46. As a maintainer, I want peak memory measured on the existing 100-asset, 20-year stress fixture, so that the complete in-memory plan is validated against the project's documented upper case.
47. As a maintainer, I want full rebuild, incremental rebuild, and warm readiness measured separately, so that one improvement does not hide a regression in another mode.
48. As a maintainer, I want exact snapshot equivalence for existing calculable scenarios, so that performance work does not change NAV values or date coverage accidentally.
49. As a maintainer, I want query-work evidence captured at the public readiness seam, so that the absence of date-scaled reads is verifiable without exposing private helpers.
50. As a maintainer, I want tests and benchmarks to use in-memory or temporary SQLite and fake Market data sources, so that verification never accesses the user database or network.
51. As an implementation agent, I want the accepted domain glossary and ADR to control terminology and trade-offs, so that implementation does not reopen settled design choices.
52. As an implementation agent, I want the prepared-plan engine to replace the current path directly after gates pass, so that no dual implementation remains to maintain.
53. As an implementation agent, I want no schema migration required, so that the optimization remains focused on preparation, execution, and persistence orchestration.
54. As an implementation agent, I want current repository range and bulk-write capabilities reused where they fit, so that new seams are introduced only for missing range shapes or instrumentation.
55. As an implementation agent, I want no public planner or executor interface created solely for tests, so that the NAV module remains deep.

## Implementation Decisions

- Introduce one immutable, NAV-owned `NavRebuildPlan` as the complete input to the calculation phase.
- The plan contains the NAV rebuild checkpoint state, ordered Transaction ledger entries, required Tracked assets, derived Positive-holding intervals, and range-loaded valuation data.
- Keep the plan and its executor private to NAV. The public readiness interface remains the caller-facing contract.
- MarketData remains the exclusive owner of source coordination, Historical market data cache policy, forward-fill behavior, source lookup identity, Market data limitations, and valuation-series construction, consistent with ADR-0001 and ADR-0002.
- NAV owns selection of performance assets, checkpoint state, ledger replay, Positive-holding interval derivation, unitization, accumulated dividend cash, contiguous-prefix calculation, Complete NAV snapshot generation, and persistence orchestration.
- Plan execution performs no database reads, repository calls, or MarketData calls. It consumes only prepared in-memory state and emits generated snapshots for persistence.
- Missing valuation data during execution is an invariant failure. Do not add a fallback repository lookup.
- Prepare one complete plan for the full rebuild range. Do not introduce date-windowed plans in this work.
- Load transactions once in ascending date and transaction-ID order for the rebuild range. Preserve same-date insertion order semantics.
- Load assets once for checkpoint holdings and assets referenced by relevant transactions.
- Derive each performance asset's Positive-holding intervals by replaying holdings state, including full closure and later reopening.
- Require a closing price only on dates where the asset has a positive end-of-day quantity that contributes to NAV.
- A date ending with zero quantity does not require an asset closing price, even when a sale occurred that day.
- Treat transaction-date FX requirements separately from closing-price intervals. Buys, sells, and dividends require the latest usable FX rate on or before their transaction date; EUR uses 1.0.
- Range-loaded price and FX series include the latest earlier observation needed as the seed for existing forward-fill and at-or-before behavior.
- Do not extend Forward-filled market data beyond the final source-supported observation.
- Compute the Effective valuation date as the last date in the contiguous calculable prefix after the NAV rebuild checkpoint.
- If a price or FX requirement first blocks a later date, persist every Complete NAV snapshot before that date and do not calculate across the blocker.
- Return the existing NAV-scoped Market data limitations describing the blocker or stale range.
- Retain existing unitization semantics for initial NAV, buys, sells, fees, dividends, splits, liquidation, and accumulated cash.
- Retain one portfolio snapshot for every calculable calendar date, including weekends and holidays supported by Forward-filled market data.
- Retain the normalized portfolio-level and per-asset history tables; no schema migration is required.
- Treat one portfolio row plus all required per-asset rows for its date as one Complete NAV snapshot.
- Persist generated output in caller-owned database transactions bounded by both a maximum date count and a maximum total generated-row target.
- Never split one Complete NAV snapshot across persistence transactions. A single date may exceed the row target when necessary.
- Repository-level statement chunks remain implementation details inside the same caller-owned transaction and must not independently commit.
- A failed persistence transaction leaves none of its dates behind. Previously committed chunks remain valid rebuild progress.
- Routine readiness trusts the latest Complete NAV snapshot as the NAV rebuild checkpoint and does not replay, audit, or repair earlier history.
- Remove the full-history completeness audit from routine readiness. Do not add an explicit verification command or compatibility layer in this work.
- Existing automatic-repair behavior for manually mutated or legacy incomplete earlier snapshots is intentionally retired.
- Concurrent write commands and multiple simultaneous rstock processes are out of scope; normal single-command usage is the operational assumption.
- Directly replace the current rebuild path after acceptance gates pass. Do not add a feature flag, shadow engine, or dual-engine period.
- Use the existing Criterion performance fixture matrix and report generation as rollout evidence, including the representative and stress NAV rebuild paths and warm readiness.
- Preserve existing fixed performance targets unless an explicit decision gate changes them; do not fabricate a new timing target from a single run.
- Update architecture and convention documentation that describes broad market-data requirements, full-history readiness auditing, Effective valuation date, or rebuild persistence behavior.

## Testing Decisions

- The single highest behavioral seam is the public NAV readiness interface. Tests call readiness and inspect returned latest snapshots, Market data limitations, and persisted Complete NAV snapshots.
- Do not expose or directly test private `NavRebuildPlan` construction, private execution helpers, internal maps, interval representations, or chunk helper functions.
- A good test verifies externally observable domain behavior: persisted date coverage, portfolio and per-asset values, Effective valuation date, limitations, atomicity, recovery, and measured database work.
- Use captured database work at the public readiness seam to prove that plan execution performs zero reads and that preparation query count does not scale with rebuilt calendar dates or holding-days.
- Query-count tests may permit work to scale with distinct assets, currencies, transactions, and persistence chunks; they must reject per-day and per-holding query growth.
- Use in-memory SQLite through the existing setup helper, dummy Tracked asset identifiers, fake `MarketDataSources`, and a fixed clock. Tests must not make network calls.
- Existing NAV integration tests are prior art for initialization, multiple buys, same-day ordering, weekends, multiple currencies, funds and ETFs, sells, fees, liquidation, splits, and per-asset history.
- Existing interruption tests are prior art for injected persistence failure, rollback of the active batch, retention of earlier complete dates, and successful resume.
- Existing repository persistence tests are prior art for caller-owned transaction rollback and atomic portfolio/per-asset writes.
- Existing market-data tests are prior art for predecessor seeds, at-or-before lookup, immutable Historical market data, Forward-filled market data, missing data, and Market data limitations.
- Existing current-date tests are prior art for the fixed latest-completed-date cutoff and readiness ownership.
- Existing Criterion fixtures are prior art for small, representative, and stress performance measurement without network or user-database access.
- Preserve exact portfolio snapshot and per-asset snapshot results for all existing calculable NAV scenarios.
- Test a closed asset whose market data ends at closure while another asset remains held later; the closed asset must not shorten the later Effective valuation date.
- Test an asset that is held, fully sold, and later repurchased; only its two Positive-holding intervals require closing prices.
- Test a full sale date with no closing price but usable transaction FX; the date remains calculable with zero end-of-day quantity.
- Test same-day buys and sells that end at zero quantity, preserving transaction-ID ordering and avoiding an unnecessary closing-price requirement.
- Test a Positive-holding interval beginning on a weekend or holiday with an earlier seed observation and a later source-supported observation.
- Test that a predecessor seed does not authorize forward-fill beyond the source-supported end.
- Test a later buy whose required price is unavailable; persist the complete prefix through the preceding date and return the limitation.
- Test a later non-EUR buy whose transaction FX is unavailable; persist the complete prefix through the preceding date and return an FX limitation.
- Test a later sell and dividend with unavailable transaction FX using the same contiguous-prefix rule.
- Test that market data available after a blocker does not allow NAV history to skip the blocked date.
- Test a fully liquidated date as a Complete NAV snapshot with no per-asset rows.
- Test the initial seed snapshot as a Complete NAV snapshot with no per-asset rows.
- Test Monetary holdings and their transactions do not create Positive-holding intervals or valuation requirements for NAV.
- Test output chunking across the date target and row target through the public readiness seam, asserting complete persisted dates rather than private chunk boundaries.
- Test a single date larger than the row target remains atomic and succeeds as one oversized persistence unit.
- Inject a late asset-row failure and assert that the entire active transaction rolls back while earlier committed Complete NAV snapshots remain.
- Retry after interruption and assert readiness resumes from the latest committed NAV rebuild checkpoint with complete, equivalent history.
- Replace tests that expect routine readiness to discover manually corrupted earlier history; the new contract trusts the checkpoint and does not perform that audit.
- Preserve tests proving that accepted Transaction ledger mutations atomically invalidate dependent Complete NAV snapshots before a later rebuild.
- Measure full, incremental, and warm NAV readiness through the existing Criterion harness.
- Measure or retain a deterministic peak-memory/allocation proxy for the complete plan on the 100-asset, 20-year stress fixture.
- Require zero source calls for fully warm Historical market data preparation, preserving the existing cache contract.
- Require `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` as final verification. Performance evidence must use the repository's offline benchmark/report procedure.

## Out of Scope

- Windowed, paged, or streaming input plans are out of scope.
- Supporting rebuild ranges materially beyond the existing 100-asset, 20-year stress fixture is out of scope unless benchmark evidence requires reconsideration.
- Concurrent NAV rebuilds, concurrent write commands, multi-process generation guards, and persisted rebuild leases are out of scope.
- A new database schema, completion marker, rebuild-generation column, staging table, or snapshot status field is out of scope.
- Replacing the normalized portfolio and per-asset history tables with JSON, event storage, or another persistence model is out of scope.
- An explicit full-history verification or repair command is out of scope.
- Automatic detection or repair of manual database edits and legacy corruption before the NAV rebuild checkpoint is out of scope.
- Feature flags, shadow calculation, dual engines, and backward-compatibility wrappers are out of scope.
- Changing NAV unitization formulas, Base currency, transaction monetary precision, or Transaction ledger chronology is out of scope.
- Using Live quotes or same-day incomplete market data in NAV is out of scope.
- Changing immutable Historical market data policy or adding a refresh operation is out of scope.
- Changing source adapters, source concurrency, Morningstar lag classification, or Individual price behavior is out of scope.
- Refactoring portfolio display, composition, correlation, fund analysis, or risk metrics is out of scope except where existing NAV consumers observe improved readiness.
- Creating public planner or executor interfaces solely for tests is out of scope.
- Establishing new hard p95 targets without an explicit performance decision gate is out of scope.

## Further Notes

- ADR-0003 records the accepted architectural decision and is authoritative for ownership, checkpoint trust, calculable-prefix behavior, persistence boundaries, memory strategy, and rollout.
- The domain glossary defines Complete NAV snapshot, NAV rebuild checkpoint, Positive-holding interval, Effective valuation date, Forward-filled market data, Transaction ledger, and related relationships. Implementation and tests should use those exact terms.
- The current implementation already preloads valuation maps and transactions for calculation and atomically batches portfolio and per-asset writes. This work should deepen and complete that direction rather than rewrite correct unitization behavior.
- The most important removed cost is the routine full-history holdings audit. Warm readiness should become proportional to checkpoint and new-range work rather than all historical per-asset rows.
- The most important semantic correction is that market-data requirements follow Positive-holding intervals and transaction dates, not every asset across one broad range.
- Output date and row targets are tuning constants, not domain promises. Complete NAV snapshot atomicity takes precedence over either target.
- The existing performance harness is the evidence source for rollout. Fixture construction remains outside timed closures, all identities remain dummy values, and no benchmark may access a network or user database.
