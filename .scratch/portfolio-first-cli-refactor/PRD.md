# PRD: Portfolio-First CLI Refactor

Status: needs-triage

## Problem Statement

rstock is a personal portfolio tracker, but the current CLI has grown command paths that blur the boundary between portfolio tracking, portfolio-relevant analysis, asset maintenance, transaction-ledger maintenance, CSV import/export, and standalone research.

The user wants the CLI to be portfolio-first. Every retained command should maintain the Transaction ledger, manage Tracked assets, explain portfolio performance or composition, analyze a Fund candidate, compare Tracked assets, or support market data needed for portfolio valuation. The current command surface violates that boundary in several places: top-level `buy` duplicates only one transaction action, `portfolio list` lists configured assets rather than portfolio holdings, `data import/export` is broad even though it only handles transaction CSVs, `monitor` is standalone stock research, and rolling correlation accepts arbitrary tickers instead of Tracked assets.

The user also wants stricter data quality. Assets entering rstock should have Asset classification at creation/import time, fund and ETF Tracked assets should require Morningstar codes, transaction CSV import/export should round-trip the metadata needed for classified assets, and transaction-entry validation should be consistent across CLI and CSV paths.

## Solution

Refactor the CLI around the portfolio-first boundary.

Keep the daily dashboard paths `get` and `portfolio get`, including the NAV chart. Remove top-level `buy` and route transaction actions through the singular `transaction` group. Remove `portfolio list` without adding an asset-list replacement. Move transaction CSV import/export from `data` into the `transaction` group. Add `transaction list` so edit/delete transaction IDs are discoverable from the CLI. Keep `transaction edit` limited to date, quantity, price, and fees; ticker and transaction-type mistakes should be corrected by delete/recreate.

Keep `analyze` as a top-level read-only group. Keep standalone fund analysis by Morningstar code, but define it as Fund candidate analysis. Change rolling correlation so it compares Tracked assets identified by the user-facing ticker/ISIN, using stock ticker lookup for stocks and stored Morningstar code lookup for funds/ETFs. Remove the monitor CLI and runtime code because it is standalone stock research, while leaving historical migrations intact.

Tighten Tracked asset creation/import. Require Asset classification at creation/import time. Enforce class-specific classification consistency. Require Morningstar code for fund and ETF Tracked assets. Keep user-facing identity as ticker for stocks and ISIN for funds/ETFs, while Morningstar code remains provider lookup metadata. Keep user-facing identity, vehicle type, and currency immutable after creation; allow descriptive classification and provider lookup metadata corrections.

Extend the transaction CSV schema with Asset classification and Morningstar code fields. Require the new schema and fail clearly for old or incomplete CSVs. Keep existing `Quantity` and `Price` column names, accepting their current type-dependent meaning because this is a personal portfolio tracker and the user prefers not to overcomplicate the schema.

## User Stories

1. As a portfolio owner, I want the CLI to stay portfolio-first, so that rstock remains focused on portfolio tracking rather than becoming a general investment terminal.
2. As a portfolio owner, I want every retained CLI feature to maintain the Transaction ledger, manage Tracked assets, explain portfolio performance, analyze a Fund candidate, compare Tracked assets, or support portfolio valuation, so that the command surface stays coherent.
3. As a portfolio owner, I want `rstock get` to remain available, so that I can quickly open my daily portfolio dashboard.
4. As a portfolio owner, I want `rstock portfolio get` to remain available, so that the dashboard is also discoverable under the portfolio namespace.
5. As a portfolio owner, I want the portfolio dashboard to continue showing the NAV chart by default, so that I can see performance trend without running a separate command.
6. As a portfolio owner, I want top-level `buy` removed, so that transaction entry uses one consistent command group.
7. As a portfolio owner, I want to record buys through `transaction buy`, so that buys live with the rest of the Transaction ledger actions.
8. As a portfolio owner, I want to record sells through `transaction sell`, so that sales remain part of the Transaction ledger.
9. As a portfolio owner, I want to record dividends through `transaction dividend`, so that total cash received for an asset is included in the Transaction ledger.
10. As a portfolio owner, I want to record splits through `transaction split`, so that quantity changes are represented explicitly.
11. As a portfolio owner, I want split ratio to continue meaning new units per old unit, so that existing split behavior stays predictable.
12. As a portfolio owner, I want dividend amount to mean total cash received, so that I can enter the amount that actually appeared in my broker account.
13. As a portfolio owner, I want transaction quantities, prices, dividend amounts, and split ratios to be positive, so that invalid Transaction ledger entries are rejected.
14. As a portfolio owner, I want transaction fees to be non-negative, so that fee handling is consistent across transaction types.
15. As a portfolio owner, I want transaction validation to be consistent for CLI entry and CSV import, so that bad data cannot enter through a different path.
16. As a portfolio owner, I want `transaction list`, so that I can discover transaction IDs before editing or deleting.
17. As a portfolio owner, I want `transaction list` to show enough transaction detail to identify the right row, so that edit/delete prompts are not my only source of confirmation.
18. As a portfolio owner, I want `transaction edit` to keep supporting date corrections, so that date entry mistakes can be fixed without recreating the transaction.
19. As a portfolio owner, I want `transaction edit` to keep supporting quantity corrections, so that quantity entry mistakes can be fixed without recreating the transaction.
20. As a portfolio owner, I want `transaction edit` to keep supporting price corrections, so that price entry mistakes can be fixed without recreating the transaction.
21. As a portfolio owner, I want `transaction edit` to keep supporting fee corrections, so that fee entry mistakes can be fixed without recreating the transaction.
22. As a portfolio owner, I want `transaction edit` not to change ticker/ISIN or transaction type, so that dangerous semantic transformations are avoided.
23. As a portfolio owner, I want ticker or transaction-type mistakes to be corrected by delete/recreate, so that validation remains clear.
24. As a portfolio owner, I want `transaction delete` to remain available, so that mistaken Transaction ledger entries can be removed deliberately.
25. As a portfolio owner, I want edit/delete confirmations to keep showing the affected transaction, so that destructive changes are reviewed before they apply.
26. As a portfolio owner, I want transaction CSV import to live under `transaction import`, so that importing is clearly about the Transaction ledger.
27. As a portfolio owner, I want transaction CSV export to live under `transaction export`, so that exporting is clearly about the Transaction ledger.
28. As a portfolio owner, I want the broad `data` group removed, so that market data and transaction CSVs are not conflated.
29. As a portfolio owner, I want transaction CSV export to include Asset classification, so that exported data can be re-imported without losing portfolio-analysis metadata.
30. As a portfolio owner, I want transaction CSV export to include Morningstar code for funds/ETFs, so that exported fund and ETF assets remain valuatable after import.
31. As a portfolio owner, I want transaction CSV import to require the new schema, so that old files do not silently create incomplete Tracked assets.
32. As a portfolio owner, I want transaction CSV import to fail clearly when classification fields are missing for a new Tracked asset, so that I know what metadata to provide.
33. As a portfolio owner, I want transaction CSV import to fail clearly when a fund or ETF is missing Morningstar code, so that NAV valuation will not fail later.
34. As a portfolio owner, I want transaction CSV import to keep using the current `Quantity` and `Price` column names, so that the personal CSV stays simple.
35. As a portfolio owner, I want the current type-dependent CSV meaning preserved, so that dividends and splits continue to import without extra amount/ratio columns.
36. As a portfolio owner, I want Tracked assets to have Asset classification at creation time, so that composition analysis has reliable inputs.
37. As a portfolio owner, I want imported Tracked assets to have Asset classification at creation time, so that import does not create weak portfolio data.
38. As a portfolio owner, I want Asset classification to distinguish vehicle type from portfolio-analysis taxonomy, so that stocks, funds, and ETFs can still be categorized by portfolio role.
39. As a portfolio owner, I want equity-specific classification fields accepted only for equity assets, so that nonsensical classifications are rejected.
40. As a portfolio owner, I want fixed-income-specific classification fields accepted only for fixed-income assets, so that bond analytics stay meaningful.
41. As a portfolio owner, I want unrelated asset classes to reject equity/bond-specific fields, so that classification remains consistent.
42. As a portfolio owner, I want fund and ETF Tracked assets to require Morningstar code, so that Historical market data can be fetched for NAV.
43. As a portfolio owner, I want stock Tracked assets not to require Morningstar code, so that stocks continue to use ticker-based market data lookup.
44. As a portfolio owner, I want ticker to remain the user-facing identity for stocks, so that stock commands stay natural.
45. As a portfolio owner, I want ISIN to remain the user-facing identity for funds/ETFs, so that fund and ETF commands use a stable asset identity.
46. As a portfolio owner, I want Morningstar code to remain provider lookup metadata, so that provider-specific identity does not replace my user-facing asset identity.
47. As a portfolio owner, I want Tracked asset identity to be immutable after creation, so that historical transactions are not reinterpreted.
48. As a portfolio owner, I want Tracked asset vehicle type to be immutable after creation, so that pricing and display semantics do not change under existing history.
49. As a portfolio owner, I want Tracked asset currency to be immutable after creation, so that historical Base currency conversion is not reinterpreted.
50. As a portfolio owner, I want Tracked asset name to remain editable, so that descriptive mistakes can be corrected.
51. As a portfolio owner, I want Tracked asset classification to remain editable, so that portfolio-analysis metadata can be corrected.
52. As a portfolio owner, I want Morningstar code to remain editable for funds/ETFs, so that provider lookup mistakes can be corrected.
53. As a portfolio owner, I want price cache invalidation when fund/ETF Morningstar code changes, so that old provider data is not reused under a new lookup identity.
54. As a portfolio owner, I want `portfolio list` removed, so that the CLI does not imply all configured assets are current portfolio holdings.
55. As a portfolio owner, I accept no replacement asset-list command for now, so that the CLI stays smaller.
56. As a portfolio owner, I want no Tracked asset delete command, so that asset history is not accidentally erased through cleanup commands.
57. As a portfolio owner, I want `analyze` to remain a top-level group, so that read-only portfolio analysis remains discoverable.
58. As a portfolio owner, I want `analyze composition` to remain available, so that I can understand portfolio allocation.
59. As a portfolio owner, I want `analyze fund --code` to remain available, so that I can analyze a Fund candidate before adding it to the portfolio.
60. As a portfolio owner, I want standalone fund analysis to be treated as Fund candidate analysis, so that it remains portfolio-relevant rather than general research.
61. As a portfolio owner, I want `analyze correlation matrix` to remain portfolio-focused, so that correlations explain existing portfolio assets and benchmark context.
62. As a portfolio owner, I want rolling correlation to compare Tracked assets, so that pair analysis can include stored stocks, funds, and ETFs without arbitrary ticker research.
63. As a portfolio owner, I want rolling correlation to use stored Morningstar code for fund/ETF Tracked assets, so that funds and ETFs can participate in rolling correlation.
64. As a portfolio owner, I want rolling correlation to reject unknown asset identifiers, so that the command does not become a general market research path.
65. As a portfolio owner, I want `monitor` removed, so that standalone stock watchlist research no longer appears in the portfolio tracker.
66. As a maintainer, I want monitor runtime code removed, so that dead code does not remain after the CLI feature is removed.
67. As a maintainer, I want historical migrations left intact, so that applied database history is not rewritten.
68. As a maintainer, I want market data cache behavior to remain implicit, so that portfolio/analyze commands continue to fetch or rebuild as needed.
69. As a maintainer, I want no market-data refresh or cache-clear CLI in this refactor, so that cache internals are not exposed without a concrete need.
70. As a maintainer, I want CLI parsing and dispatch simplified, so that command definitions match the portfolio-first model.
71. As a maintainer, I want a deep Asset classification validation module, so that class-specific validation is testable outside command parsing.
72. As a maintainer, I want a deep transaction CSV contract module, so that schema validation, row parsing, and export shape are tested through a stable interface.
73. As a maintainer, I want transaction listing implemented through service/repository seams, so that display remains presentation-only.
74. As a maintainer, I want repositories to remain pure data access, so that business rules do not move into database modules.
75. As an implementation agent, I want the architecture docs updated for the CLI command surface, so that future work does not follow stale command descriptions.
76. As an implementation agent, I want the domain glossary preserved, so that terms like Tracked asset, Fund candidate, Asset classification, and Transaction ledger are used consistently.

## Implementation Decisions

- Modify the CLI command definitions to remove top-level `buy`, remove `portfolio list`, remove `data`, add `transaction list`, and add `transaction import/export`.
- Keep `get` and `portfolio get` as duplicate dashboard paths.
- Keep the portfolio dashboard chart always enabled; no `--no-chart` or separate chart command is planned.
- Keep the singular `transaction` command group.
- Keep `transaction edit` limited to date, quantity, price, and fees.
- Add transaction-listing behavior that exposes transaction IDs and enough identifying detail for edit/delete workflows.
- Move transaction CSV import/export command dispatch under the transaction command group.
- Extend the transaction CSV contract to include Asset classification fields and Morningstar code.
- Require the new transaction CSV schema; old 9-column CSVs are not supported by this refactor.
- Preserve current CSV `Quantity` and `Price` column names and their type-dependent semantics.
- Require Asset classification for manual Tracked asset creation and import-created Tracked assets.
- Enforce class-specific Asset classification consistency for create/import/edit.
- Require Morningstar code for fund and ETF Tracked assets at creation/import time.
- Do not require Morningstar code for stock Tracked assets.
- Keep ticker/ISIN, asset type, and currency immutable after Tracked asset creation.
- Allow name, Asset classification, and Morningstar code edits.
- Invalidate relevant cached price data when a fund/ETF Morningstar code changes.
- Keep no asset-list replacement after removing `portfolio list`.
- Add no Tracked asset delete command.
- Keep `analyze` as the top-level analysis group.
- Keep standalone fund analysis by Morningstar code as Fund candidate analysis.
- Change rolling correlation to resolve both requested identifiers as Tracked assets.
- Allow rolling correlation for stock, fund, and ETF Tracked assets when market data lookup identity is available.
- Remove monitor from the CLI command surface.
- Remove monitor runtime modules, models, services, display code, repository exposure, constants, and tests that become dead after removing the feature.
- Leave historical database migrations intact, including migrations that created monitor/watchlist schema.
- Do not add market data refresh, clear, or cache-management commands.
- Update architecture/conventions documentation that describes the CLI command surface.
- Good deep-module candidates are Asset classification validation and transaction CSV contract handling because they encapsulate several rules behind small, testable interfaces.
- Transaction-listing service behavior should remain separate from display formatting.
- Repository modules should remain pure data access modules and should not own Asset classification, CSV, or transaction validation rules.

## Testing Decisions

- Good tests should exercise external behavior through command-adjacent service/module interfaces rather than private helpers.
- Good CLI tests should assert accepted/rejected command shapes and visible behavior, not implementation-specific match arms.
- Test Asset classification validation as a deep module through its public validation interface.
- Test that equity-specific attributes are accepted for equity assets and rejected for incompatible top-level asset classes.
- Test that fixed-income-specific attributes are accepted for fixed-income assets and rejected for incompatible top-level asset classes.
- Test that manual Tracked asset creation rejects missing Asset classification.
- Test that manual fund/ETF Tracked asset creation rejects missing Morningstar code.
- Test that stock Tracked asset creation does not require Morningstar code.
- Test that Tracked asset edits preserve immutable ticker/ISIN, asset type, and currency behavior.
- Test that Morningstar code edits invalidate stale provider price data for funds/ETFs.
- Test transaction-entry validation for positive quantities, prices, dividend amounts, split ratios, and non-negative fees.
- Test transaction CSV import rejects the old 9-column schema.
- Test transaction CSV import creates fully classified Tracked assets from the new schema.
- Test transaction CSV import rejects missing classification for new assets.
- Test transaction CSV import rejects missing Morningstar code for new fund/ETF assets.
- Test transaction CSV export writes the new schema and enough metadata for round-trip import.
- Test transaction CSV import preserves current `Quantity` and `Price` type-dependent semantics for buy, sell, dividend, and split rows.
- Test `transaction list` returns transaction IDs and identifying transaction details in date order.
- Test `transaction list` filtering if filters are implemented.
- Test edit/delete workflows continue to locate transactions by ID after listing is added.
- Test rolling correlation rejects unknown identifiers that are not Tracked assets.
- Test rolling correlation uses ticker lookup for stock Tracked assets.
- Test rolling correlation uses Morningstar code lookup for fund/ETF Tracked assets.
- Test rolling correlation can compare a stock Tracked asset and a fund/ETF Tracked asset when both have usable market data.
- Test removed command paths are no longer accepted where the project has CLI parsing coverage.
- Test monitor runtime removal by deleting or updating monitor-specific tests rather than preserving unreachable behavior.
- Prior art exists in the existing transaction service tests, import/export service tests, asset-classification tests, correlation tests, and monitor tests that will be removed or replaced.
- Use in-memory SQLite via the existing test setup.
- Use dummy tickers and avoid network calls.
- Use the existing mock price fetcher approach for rolling correlation and fund/ETF market data behavior.

## Out of Scope

- No implementation work is included in this PRD.
- No GitHub issue creation is included; this repo tracks PRDs and issues as local markdown under `.scratch/`.
- No market-data cache refresh, cache clear, or manual rebuild command will be added.
- No asset-list replacement command will be added after removing `portfolio list`.
- No Tracked asset delete command will be added.
- No transaction type or ticker/ISIN mutation through `transaction edit` will be added.
- No split from/to field redesign will be added; split ratio semantics remain unchanged.
- No dividend per-share entry mode will be added; dividends remain total cash received.
- No CSV backward compatibility with the old 9-column schema will be maintained.
- No migration rewrite will remove historical watchlist schema.
- No standalone stock research or watchlist replacement will be introduced.
- No ADR is required at this point because the decisions are CLI/domain-surface refactors rather than hard-to-reverse architectural commitments.

## Further Notes

The domain glossary has already been updated with the resolved vocabulary: Portfolio-relevant analysis, Fund candidate, Asset classification, Tracked asset, and Transaction ledger.

There is a known documentation mismatch discovered during exploration: architecture docs mention old market-data module names in places. The CLI refactor should update stale CLI and module descriptions touched by this work.

The PRD intentionally keeps the personal-tool bias: strict enough to prevent bad portfolio data, but not overdesigned for external CSV consumers or public API compatibility.
