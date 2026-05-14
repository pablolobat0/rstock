Status: needs-triage

# PRD: Refactor Test Suite Around Domain Behavior

## Problem Statement

The current test suite is difficult to navigate and reason about. Test files are organized inconsistently: some follow Rust module names, some follow feature names, and one broad integration file mixes unrelated scenarios. Many test names look arbitrary or generated, even when the tests are hand-written, because they use inconsistent naming styles and scenario data. Setup is repeated across files through low-level insert helpers, which makes domain intent harder to see and increases the cost of adding coverage for **Transaction ledger**, **NAV**, **Historical market data**, **Effective valuation date**, **Forward-filled market data**, **Market data limitation**, **Tracked asset**, and **Portfolio-relevant analysis** behavior.

The existing conventions also now conflict with the desired direction. The documented testing pattern says to avoid fixtures and build every test state imperatively, but the current suite has enough repetition that small shared fixtures and scenario builders would improve clarity. Some tests depend on the real clock, especially stale market data warning scenarios, which makes expected behavior harder to understand and can make tests fragile over time.

## Solution

Refactor the tests into a domain-oriented suite with six broad integration test files, a split common test support module, deterministic fixture data, service-backed scenario builders, and explicit assertion helpers. The refactor should improve structure and coverage together: low-signal or duplicate tests can be merged or deleted when their behavior is covered by clearer domain scenarios, and new coverage should prioritize domain invariants rather than adding arbitrary permutations.

From the user's perspective, the result is a test suite where each file answers a clear question: how the **Transaction ledger** behaves, how **NAV** valuation behaves, how market data preparation behaves, how portfolio views behave, how **Portfolio-relevant analysis** behaves, and how low-level model/math invariants behave. Tests should read like behavior specifications, use stable fake assets and dates, avoid real network calls, and make financial expectations visible through named assertion helpers.

## User Stories

1. As a maintainer, I want tests grouped by domain behavior, so that I can find the relevant coverage without knowing the implementation module names.
2. As a maintainer, I want **Transaction ledger** tests in a clear ledger area, so that buy, sell, dividend, split, import, edit, and delete behavior is easy to review together.
3. As a maintainer, I want **NAV** and valuation tests in a clear valuation area, so that cash-flow and portfolio-history behavior is easy to understand.
4. As a maintainer, I want **Historical market data** tests in a clear market data area, so that price, FX, **Effective valuation date**, and **Forward-filled market data** behavior is easy to reason about.
5. As a maintainer, I want portfolio view tests separated from valuation internals, so that current position display data and warning behavior are not mixed with NAV unitization tests.
6. As a maintainer, I want **Portfolio-relevant analysis** tests grouped together, so that composition, correlation, fund analysis, metrics, and monitor behavior can be reviewed as analysis behavior.
7. As a maintainer, I want low-level pure model and math tests grouped consistently under integration tests, so that the suite does not mix inline and external test styles.
8. As a developer adding a feature, I want behavior-style test names without a redundant prefix, so that failures describe the broken behavior directly.
9. As a developer adding a test, I want canonical fake assets, so that fixture data does not feel random.
10. As a developer adding a test, I want canonical fixed dates, so that date-dependent behavior is deterministic.
11. As a developer adding a test, I want the default fake assets to use impossible tickers, so that no test accidentally performs real provider lookups.
12. As a developer adding a valuation test, I want a small scenario builder for common asset, ledger, price, FX, and rebuild setup, so that the scenario intent is visible.
13. As a developer adding a ledger test, I want normal workflows to go through production services by default, so that validation and invalidation behavior are covered.
14. As a developer adding a persistence-focused test, I want raw database insert helpers to remain available, so that I can create exact or intentionally impossible states.
15. As a developer adding a market data test, I want a configurable mock fetcher in a focused helper module, so that provider behavior is explicit and no network calls occur.
16. As a developer adding a portfolio view test, I want helpers for market data warnings, so that **Market data limitation** expectations are readable.
17. As a developer adding an analysis test, I want reusable fixture setup for **Tracked asset** classification and portfolio history, so that analysis behavior is not obscured by persistence boilerplate.
18. As a developer reviewing a failure, I want assertion helpers for money values, NAV values, ratios, and identifiers, so that floating-point failures are reported consistently.
19. As a developer reviewing a financial test, I want expected values named in the test, so that I can see the invariant being asserted.
20. As a developer reviewing market data behavior, I want tests to use glossary terms such as **Historical market data**, **Live quote**, and **Effective valuation date**, so that test names match the domain model.
21. As a developer reviewing stale data behavior, I want tests to avoid reading the real current date where feasible, so that results do not change with the calendar.
22. As a developer maintaining test helpers, I want common test code split by responsibility, so that database setup, builders, assertions, and fetcher behavior do not become one shallow utility file.
23. As a developer maintaining test helpers, I want builders to expose a small stable interface, so that tests are insulated from incidental database details.
24. As a developer maintaining test helpers, I want the builder modules to be deep modules, so that they encapsulate repetitive setup behind simple methods that rarely change.
25. As a maintainer, I want duplicate or low-signal tests merged into clearer scenarios, so that test volume supports confidence rather than noise.
26. As a maintainer, I want removed tests accounted for by equivalent behavior coverage during the refactor, so that confidence is not lost.
27. As a maintainer, I want new coverage focused on domain invariants, so that the suite protects financial behavior rather than arbitrary implementation details.
28. As a maintainer, I want **NAV** continuity across cash flows covered clearly, so that deposits, sells, dividends, and splits preserve expected performance semantics.
29. As a maintainer, I want no-partial-snapshot failure behavior covered clearly, so that failed valuation does not leave misleading history.
30. As a maintainer, I want **Forward-filled market data** coverage to state that fills never extend beyond source availability, so that valuation remains reproducible.
31. As a maintainer, I want **Transaction ledger** source-of-truth behavior covered clearly, so that holdings and CSV import/export semantics remain trustworthy.
32. As a maintainer, I want **Base currency** and non-base currency valuation coverage to be readable, so that EUR aggregation behavior remains protected.
33. As a maintainer, I want fund and ETF Morningstar-code market data behavior covered with domain names, so that lookup identity rules are visible.
34. As a maintainer, I want **Live quote** behavior separated from **Historical market data** behavior, so that same-day display behavior does not contaminate NAV expectations.
35. As a maintainer, I want benchmark market data tests to remain distinct from holdings tests, so that benchmark behavior is not confused with portfolio assets.
36. As a developer running tests locally, I want the suite to remain reasonably fast, so that clarity improvements do not make normal development painful.
37. As a developer running focused tests locally, I want broad file names that match the behavior area, so that `cargo test` filters are intuitive after renaming.
38. As a future contributor, I want the existing testing conventions updated, so that documentation does not tell me to avoid the scenario builders the suite now depends on.
39. As a future contributor, I want no separate test README required, so that the code structure and existing conventions remain the primary guide.
40. As a maintainer, I want the refactor implemented incrementally by domain bucket, so that failures can be attributed to a small set of moves and rewrites.

## Implementation Decisions

- Organize the top-level integration suite into six broad domain buckets: ledger, valuation, market data, portfolio view, analysis, and model invariants.
- Keep pure model and math tests under the integration test suite rather than moving them inline beside implementation code.
- Rename tests to behavior-style names without a redundant test prefix.
- Align test names, helper names, and scenario terminology with the existing domain glossary where it improves clarity.
- Split common test support into focused responsibilities: database setup and raw persistence helpers, scenario builders, assertion helpers, and mock price-fetching helpers.
- Treat the scenario builders as deep test modules: they should hide repetitive setup behind stable, intention-revealing interfaces.
- Make scenario builders service-backed by default for normal workflows, especially **Transaction ledger** behavior, so that production validation and invalidation paths are exercised.
- Preserve raw database helpers for tests that intentionally focus on persistence, precomputed history, or impossible states that normal services should not create.
- Define canonical fake **Tracked asset** identities for common EUR stock, USD stock, EUR fund, and EUR ETF scenarios.
- Define canonical fixed date windows, including dates suitable for weekday and weekend/forward-fill scenarios.
- Remove real-clock dependence where feasible. If production code currently reads the current date internally, add the smallest seam needed only where deterministic tests require it.
- Merge or delete duplicate and low-signal tests when the same behavior is covered by a clearer scenario.
- Do not preserve old test names for compatibility with local filters.
- Use simple table-driven loops only for pure or low-setup cases. Keep complex database-backed domain scenarios as separate named tests.
- Avoid generated, randomized, or property-based tests in this refactor.
- Update the existing testing conventions documentation to reflect the new structure and remove the stale no-fixtures guidance.
- Do not add a separate test README.
- No database schema changes are required.
- No CLI or external API contract changes are required.
- No ADR is required because this is reversible test architecture work and can be understood from the updated conventions.

## Testing Decisions

- A good test should verify external domain behavior rather than incidental implementation details.
- A good test should read as a scenario using domain vocabulary, with setup and assertions that make the behavior under test obvious.
- A good financial test should use deterministic data and named expectations rather than unexplained arithmetic embedded in comments.
- A good market data test should distinguish **Historical market data**, **Live quote**, **Forward-filled market data**, **Stale market data**, and **Market data limitation** behavior explicitly.
- Floating-point assertions should use domain-specific helpers: exact equality for identifiers and integer/cents values, cent-level checks for monetary values, and explicit tolerances for NAV, ratios, and metrics.
- The ledger bucket should cover **Transaction ledger** behavior, including buys, sells, dividends, splits, import/export, edit/delete, validation, and invalidation effects.
- The valuation bucket should cover **NAV** unitization, portfolio history rebuilds, outstanding shares, asset snapshots, dividends, splits, currency conversion, failure atomicity, and **Effective valuation date** consequences.
- The market data bucket should cover price caching, FX caching, provider identity rules, **Forward-filled market data**, stale-data classification, **Live quote** separation, benchmark data, and no-network mock behavior.
- The portfolio view bucket should cover current positions, returns, **Individual price** behavior, market data warning surfacing, acceptable Morningstar lag, and stale stock/FX warnings.
- The analysis bucket should cover composition, correlation, fund analysis, metrics, monitor reports, and **Portfolio-relevant analysis** semantics.
- The model bucket should cover pure conversion and math invariants that are not naturally part of a larger domain scenario.
- Existing prior art includes the current integration tests for NAV unitization, market data preparation, daily price lookup, portfolio summaries, dividends, correlation, composition, fund analysis, metrics, monitor reports, asset classification, transactions, import, edit, and delete behavior.
- The refactor should run verification incrementally by bucket where feasible, then finish with the repository verification commands.
- Required final verification remains `cargo fmt && cargo clippy -- -D warnings` and `cargo test`.

## Out of Scope

- Changing production portfolio, valuation, market data, ledger, or analysis behavior except for minimal deterministic test seams where needed.
- Adding property-based testing or randomized/generated tests.
- Adding snapshot or golden-file tests as the primary assertion style.
- Adding a separate test README.
- Preserving old test names for compatibility with local `cargo test` filters.
- Changing database schema, migrations, CLI behavior, user-facing output, or external provider behavior.
- Refactoring production modules solely for test convenience unless a small seam is needed to eliminate real-clock dependence.
- Creating an ADR for the test-suite refactor.

## Further Notes

- The existing testing conventions currently state that tests should not use fixtures. That convention should be replaced with guidance for small, composable scenario builders and canonical deterministic fixture data.
- The implementation should proceed incrementally: common helpers first, then valuation, market data, ledger, portfolio view, analysis, and model buckets.
- Clarity is more important than preserving exact test count. The suite may end with fewer tests if merged scenarios cover the same behavior more clearly.
- Runtime should remain in the same general order of magnitude as the current suite, but modest increases are acceptable if the tests become substantially clearer.
