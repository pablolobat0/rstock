# PRD: Fund Analysis Comparison

Status: needs-triage

## Problem Statement

rstock can analyze one **Fund candidate** by Morningstar code, but the analysis does not yet answer the most important portfolio-fit questions. A portfolio owner cannot see how a candidate fund correlates with the whole portfolio **NAV**, how it correlates with each current holding, or whether the requested correlation period is fully supported by available history.

rstock also lacks a first-class way to compare two **Fund candidate** values side by side. The user wants to compare the same performance and composition dimensions already present in fund analysis, but focused on fund-vs-fund differences: shared holdings, side-by-side allocations, fund-to-fund correlation, and an aligned return graph. The comparison should stay portfolio-relevant while still allowing untracked Morningstar codes, matching the current behaviour of single-fund analysis.

The existing Morningstar holdings endpoint also does not supply all desired fund-level facts. The user specifically wants **Fund quote metadata** for fund name fallback, AUM, inception date, and quote currency, fetched from a separate Morningstar quote endpoint without replacing the current holdings endpoint.

## Solution

Extend single-fund analysis with **Fund quote metadata** and **Fund candidate correlation**. `analyze fund` will keep accepting a Morningstar code and will keep working for untracked candidates. It will add a `--period` flag, defaulting to `1y`, used only for the new correlation section. Existing YTD, 1Y, 3Y, 5Y, and all-time performance metrics remain unchanged. The new section appears at the end of the report and shows correlation against whole portfolio **NAV** first, then against each currently held **Tracked asset** sorted by correlation descending, with unavailable rows shown as `N/A` plus a short reason.

Add a new top-level `compare` command group with `funds` as the initial subcommand. `rstock compare funds --code-a X --code-b Y --period 1y` compares two Morningstar fund codes. It allows untracked codes, rejects identical codes, fetches holdings, quote metadata, and price history for both funds, then displays fund info, multi-period performance metrics, **Common fund holding** rows, allocation comparisons, fund-to-fund correlation, and an aligned return graph. The command updates holdings snapshot history for both funds but does not display snapshot diffs.

Add Morningstar quote metadata fetching behind the existing market data Module. This respects the existing ADR that the market data Module owns source coordination and private Morningstar source **Adapters**. The quote endpoint is a second Morningstar call alongside the current holdings call; it does not replace holdings data and does not persist quote metadata.

## User Stories

1. As a portfolio owner, I want single-fund analysis to show AUM, so that I can understand the scale of a **Fund candidate**.
2. As a portfolio owner, I want single-fund analysis to show inception date, so that I can understand how long the fund has existed.
3. As a portfolio owner, I want AUM displayed with the fund currency, so that the amount has useful context.
4. As a portfolio owner, I want AUM formatted like other rstock numbers, so that the CLI output remains consistent.
5. As a portfolio owner, I want inception date displayed as `DD-MM-YYYY`, so that it matches other user-facing dates.
6. As a portfolio owner, I want quote currency to override holdings currency when available, so that the displayed currency uses the fund-level quote source.
7. As a portfolio owner, I want quote metadata to show `N/A` when unavailable, so that missing AUM or inception date is explicit.
8. As a portfolio owner, I want single-fund analysis to use a local tracked-asset name when one exists, so that my preferred naming remains visible.
9. As a portfolio owner, I want single-fund analysis to use Morningstar `investmentName` when no local asset exists, so that untracked **Fund candidate** reports are not labeled `Unknown Fund` when the source provides a name.
10. As a portfolio owner, I want single-fund analysis to fall back to `Unknown Fund` and the code when no name is available, so that the report still identifies the requested fund.
11. As a portfolio owner, I want quote metadata failure not to fail the full fund analysis, so that holdings and performance analysis remain available.
12. As a portfolio owner, I want holdings failures to fail fund analysis, so that incomplete reports do not hide missing core fund data.
13. As a portfolio owner, I want fund price history failures to fail fund analysis, so that performance and correlation are not computed from absent data.
14. As a portfolio owner, I want benchmark failures to keep beta unavailable rather than fail the whole report, so that the rest of fund analysis can still render.
15. As a portfolio owner, I want `analyze fund` to accept `--period`, so that I can choose the correlation window.
16. As a portfolio owner, I want `analyze fund --period` to default to `1y`, so that normal analysis has a useful correlation window without extra flags.
17. As a portfolio owner, I want `analyze fund --period` to accept `30d`, `6m`, `1y`, `3y`, and `5y`, so that it matches existing correlation vocabulary.
18. As a portfolio owner, I want existing YTD, 1Y, 3Y, 5Y, and all-time fund metrics to remain unchanged, so that adding correlations does not change existing analysis meaning.
19. As a portfolio owner, I want **Fund candidate correlation** against whole portfolio **NAV**, so that I can judge whether a candidate behaves like my current portfolio.
20. As a portfolio owner, I want **Fund candidate correlation** against each currently held **Tracked asset**, so that I can see which holdings the candidate behaves like.
21. As a portfolio owner, I want portfolio correlation to use **NAV**, so that deposits and sells do not distort the relationship.
22. As a portfolio owner, I want held-asset correlations to use **Base currency** price returns, so that transaction quantity changes do not distort the relationship.
23. As a portfolio owner, I want correlations calculated from aligned daily log returns, so that the metric follows existing rstock correlation semantics.
24. As a portfolio owner, I want candidate correlations displayed at the end of single-fund analysis, so that the existing report flow remains intact.
25. As a portfolio owner, I want the candidate correlation section title to include the selected period, so that the numbers are not read out of context.
26. As a portfolio owner, I want portfolio **NAV** listed first in candidate correlations, so that the headline relationship is easy to find.
27. As a portfolio owner, I want held-asset correlations sorted descending, so that the strongest relationships appear first.
28. As a portfolio owner, I want unavailable correlation rows shown as `N/A`, so that missing values are not confused with omitted holdings.
29. As a portfolio owner, I want unavailable correlation rows to include short reasons, so that I know whether the portfolio or asset lacks coverage.
30. As a portfolio owner, I want a portfolio rebuild attempted before candidate correlations, so that correlations use current portfolio history when possible.
31. As a portfolio owner, I want portfolio rebuild failures to be non-fatal for fund analysis, so that a fund report can still be useful without portfolio correlations.
32. As a portfolio owner, I want candidate correlations to require full requested period coverage, so that `5y` means a five-year relationship rather than whatever overlap exists.
33. As a portfolio owner, I want coverage checks to tolerate weekends and holidays around the requested start date, so that valid market data is not rejected for calendar artifacts.
34. As a portfolio owner, I want coverage checks to tolerate up to seven calendar days around the requested start date, so that normal source cadence does not create false unavailability.
35. As a portfolio owner, I want coverage checks to require the series end to be within seven calendar days of the requested end date, so that stale data does not appear current.
36. As a portfolio owner, I want candidate correlations to use the latest portfolio **NAV** date as the requested end, so that normal **Effective valuation date** limits do not make valid correlations unavailable.
37. As a portfolio owner, I want correlation coefficients displayed as decimals, so that they match the existing correlation matrix.
38. As a portfolio owner, I want a `compare` command group, so that fund comparison has a clear top-level home.
39. As a portfolio owner, I want `rstock compare funds --code-a X --code-b Y --period 1y`, so that I can compare two funds directly.
40. As a portfolio owner, I want fund comparison to allow untracked Morningstar codes, so that I can compare candidate funds before adding them to my portfolio.
41. As a portfolio owner, I want fund comparison to use local tracked-asset names when available, so that my preferred names are used.
42. As a portfolio owner, I want fund comparison to use Morningstar names when local names are unavailable, so that untracked comparisons still have readable labels.
43. As a portfolio owner, I want fund comparison to fall back to Morningstar codes when no name is available, so that every table has a stable label.
44. As a portfolio owner, I want comparison tables to use full fund names without codes, so that tables are readable.
45. As a portfolio owner, I want the comparison identity section to include fund codes, so that similar share classes remain distinguishable.
46. As a portfolio owner, I want `compare funds` to reject identical fund codes, so that accidental self-comparisons do not produce trivial output.
47. As a portfolio owner, I want `compare funds --period` to default to `1y`, so that graph and correlation work without extra flags.
48. As a portfolio owner, I want `compare funds --period` to accept `30d`, `6m`, `1y`, `3y`, and `5y`, so that period selection matches other correlation commands.
49. As a portfolio owner, I want the comparison period to control only fund-to-fund correlation and the aligned return graph, so that performance metrics keep their standard multi-period view.
50. As a portfolio owner, I want fund comparison performance metrics for YTD, 1Y, 3Y, 5Y, and all time, so that comparison matches single-fund analysis.
51. As a portfolio owner, I want each performance metric cell to show `N/A` independently when a fund lacks that period, so that shorter valid periods remain visible.
52. As a portfolio owner, I want comparison beta to remain versus the configured benchmark, so that beta keeps the same meaning as single-fund analysis.
53. As a portfolio owner, I want fund-to-fund correlation separate from benchmark beta, so that different relationship metrics are not overloaded.
54. As a portfolio owner, I want fund comparison to display fund info first, so that I can identify and contextualize the funds.
55. As a portfolio owner, I want fund info comparison to include currency, AUM, inception date, total holdings, top 10 weight, and portfolio date, so that fund-level facts are side by side.
56. As a portfolio owner, I want fund info comparison to show `N/A` for unavailable AUM and inception date, so that missing quote metadata is visible.
57. As a portfolio owner, I want performance comparison after fund info, so that returns and risk are easy to compare.
58. As a portfolio owner, I want performance comparison rows organized by metric and fund, with periods as columns, so that it resembles existing single-fund analysis.
59. As a portfolio owner, I want comparison to replace top 30 holdings with **Common fund holding** rows, so that overlap is the focus of the comparison.
60. As a portfolio owner, I want common holdings computed from the holdings of both funds, so that overlap is based on reported fund contents.
61. As a portfolio owner, I want a common holding to match by ticker when both sides have one, so that the strongest available identity is used.
62. As a portfolio owner, I want a common holding to match by normalized name when tickers are absent, so that holdings without tickers can still be compared.
63. As a portfolio owner, I want name normalization to trim whitespace, ignore case, and collapse repeated spaces, so that harmless source formatting differences do not prevent matching.
64. As a portfolio owner, I do not want fuzzy matching initially, so that similar but different security names are not incorrectly merged.
65. As a portfolio owner, I want common holdings sorted by the larger fund weight descending, so that the most important overlaps appear first.
66. As a portfolio owner, I want common holdings to include all reported holding types, so that cash, bonds, derivatives, and equities can all be compared when present.
67. As a portfolio owner, I want common holdings to show ticker, Fund A holding name, Fund A weight, Fund B holding name, and Fund B weight, so that matched differences are transparent.
68. As a portfolio owner, I want missing tickers shown as `—`, so that common-holdings rows match existing optional-field display style.
69. As a portfolio owner, I want fund comparison to fetch up to 200 holdings per fund, so that it matches current single-fund analysis data depth.
70. As a portfolio owner, I want sector allocation comparison side by side, so that allocation differences are visible.
71. As a portfolio owner, I want country allocation comparison side by side, so that geographic differences are visible.
72. As a portfolio owner, I want currency allocation comparison side by side, so that currency exposure differences are visible.
73. As a portfolio owner, I want allocation comparison rows to use the union of both funds' categories, so that categories present in only one fund are still shown.
74. As a portfolio owner, I want missing allocation category weights displayed as `0,00%`, so that absent categories are explicit.
75. As a portfolio owner, I want allocation rows sorted by the larger fund weight descending, so that the most material allocation differences appear first.
76. As a portfolio owner, I want comparison allocation columns to be category, Fund A weight, and Fund B weight, so that tables are predictable.
77. As a portfolio owner, I want fund-to-fund correlation displayed near the end, so that fund identity, performance, and composition come first.
78. As a portfolio owner, I want the fund-to-fund correlation section to include the selected period, so that the relationship window is clear.
79. As a portfolio owner, I want fund-to-fund correlation to require full selected-period coverage, so that `5y` does not silently become a shorter overlap.
80. As a portfolio owner, I want no fallback graph when selected-period coverage is unavailable, so that the graph does not misrepresent the requested period.
81. As a portfolio owner, I want correlation unavailability to show a short reason, so that I know which fund lacks coverage.
82. As a portfolio owner, I want the correlation value shown above the graph, so that the headline metric is visible before the visual context.
83. As a portfolio owner, I want an aligned return graph with both funds starting at `0%`, so that I can visually compare return paths.
84. As a portfolio owner, I want graph cumulative returns calculated as price relative to the first shared price, so that the chart is easy to understand.
85. As a portfolio owner, I want correlation calculated from daily log returns while the graph uses price-relative cumulative returns, so that each output uses the clearest calculation for its purpose.
86. As a portfolio owner, I want the aligned return graph to be an ASCII terminal chart, so that it fits the current CLI.
87. As a portfolio owner, I want the graph to overlay both series with a legend and start/end summaries, so that I can interpret the two lines.
88. As a portfolio owner, I want fund comparison to update holdings snapshot history for both funds, so that comparison also refreshes longitudinal fund monitoring data.
89. As a portfolio owner, I do not want fund comparison to show holdings snapshot diffs, so that the comparison report stays focused on fund-vs-fund analysis.
90. As a portfolio owner, I want snapshot identity to remain Morningstar code plus reported portfolio date, so that repeated analysis of the same snapshot does not duplicate history.
91. As a portfolio owner, I want snapshot fingerprints to keep using holding name and weight, so that comparison diffs match existing fund analysis behaviour.
92. As a portfolio owner, I want snapshot weight changes detected above the existing tolerance, so that tiny source noise does not become a change.
93. As a portfolio owner, I want fund comparison not to trigger a portfolio rebuild, so that fund-vs-fund comparison is not blocked by unrelated portfolio valuation work.
94. As a portfolio owner, I want fund comparison to fail if either fund's holdings data cannot be fetched, so that common holdings and allocations are not incomplete.
95. As a portfolio owner, I want fund comparison to fail if either fund's price history cannot be fetched, so that performance, graph, and correlation are not misleading.
96. As a portfolio owner, I want quote metadata failures not to fail comparison, so that core fund comparison can still render.
97. As a maintainer, I want quote metadata fetching behind market data, so that Morningstar source details do not leak into fund analysis or comparison services.
98. As a maintainer, I want a dedicated quote URL configuration value, so that the quote endpoint is not coupled to the holdings endpoint.
99. As a maintainer, I want quote endpoint query parameters kept private to the Morningstar adapter, so that source-specific request details do not become domain configuration.
100. As a maintainer, I want the quote endpoint to use `locale=en`, `clientId=MDC`, `benchmarkId=mstarorcat`, and `version=4.71.0`, so that the request shape matches the verified working endpoint.
101. As a maintainer, I want quote metadata parsed into a small model with name, AUM, AUM currency, and normalized inception date, so that callers receive source-neutral fund facts.
102. As a maintainer, I want AUM represented as an optional numeric value, so that integer and decimal API values can be handled.
103. As a maintainer, I want inception date normalized to internal `YYYY-MM-DD`, so that display formatting can remain consistent.
104. As a maintainer, I want no SQLite persistence for quote metadata, so that live fund-level facts do not contaminate NAV history or the Transaction ledger.
105. As a maintainer, I want quote metadata excluded from holdings snapshot fingerprints, so that AUM changes do not appear as holdings changes.
106. As a maintainer, I want fund comparison logic extracted behind a deep service interface, so that common holdings, side-by-side metrics, and graph inputs can be tested without display coupling.
107. As a maintainer, I want common holdings matching isolated in a testable module or helper, so that matching rules can evolve without touching CLI formatting.
108. As a maintainer, I want display code to remain presentation-only, so that calculations stay in services and models.
109. As a maintainer, I want repositories to remain pure data access modules, so that snapshot business rules remain in services.
110. As an implementation agent, I want command parsing tests for new and changed command shapes, so that CLI regressions are caught.
111. As an implementation agent, I want common holdings tests for ticker and normalized-name matching, so that the main comparison risk is covered.
112. As an implementation agent, I want no network calls in tests, so that the suite remains deterministic.

## Implementation Decisions

- Modify `analyze fund` to accept an optional correlation period flag with the same period labels as existing correlation commands and a default of `1y`.
- Keep `analyze fund` metrics for YTD, 1Y, 3Y, 5Y, and all time unchanged; the new period flag affects only **Fund candidate correlation**.
- Preserve current single-fund behaviour that accepts arbitrary Morningstar codes and does not require a **Tracked asset**.
- Add fund name fallback from **Fund quote metadata** when no local tracked-asset name exists.
- Keep local tracked-asset name ahead of Morningstar name when both are available.
- Add quote metadata fields to the single-fund analysis result: name fallback, AUM, AUM currency, inception date, and quote currency.
- Display single-fund header fields in this order: currency, AUM, inception, total holdings, top 10 weight, portfolio date.
- Always show AUM and inception fields in fund analysis, using `N/A` when unavailable.
- Format AUM with the existing European number style rather than compact suffixes.
- Normalize and display inception date using the app's internal and display date conventions.
- Add a **Fund candidate correlation** result that contains a portfolio **NAV** row plus rows for currently held **Tracked asset** values.
- Trigger a portfolio rebuild before computing candidate correlations, but treat rebuild/correlation failures as non-fatal for the rest of single-fund analysis.
- Use portfolio **NAV** returns for whole-portfolio correlation.
- Use **Base currency** price returns for individual held-asset correlations.
- Include only assets currently held in the latest portfolio snapshot for individual held-asset correlations.
- Sort candidate correlation output with portfolio **NAV** first, then available held assets by descending correlation, and unavailable rows last.
- Show unavailable candidate correlations as `N/A` with reasons.
- Require full requested-period coverage for candidate correlation rows, with seven-day start and end tolerance for normal source cadence.
- Use the latest portfolio **NAV** date as the requested end date for candidate correlations.
- Add a new top-level `compare` command group with a `funds` subcommand.
- `compare funds` accepts `--code-a`, `--code-b`, and `--period`, with the same period labels and default `1y`.
- `compare funds` rejects identical Morningstar codes.
- `compare funds` allows untracked Morningstar codes.
- `compare funds` should not trigger a portfolio rebuild.
- Fetch quote metadata, holdings, and price history for both compared funds concurrently where practical.
- Fail `compare funds` if either fund's holdings data or price history cannot be fetched.
- Treat quote metadata failure as non-fatal for both single-fund analysis and comparison, logging a warning and displaying `N/A` where relevant.
- Add a fund comparison result model that contains fund identities, quote metadata, period metrics for both funds, common holdings, allocation comparisons, correlation/graph data, and snapshot diff data for both funds.
- Use full fund names without codes in comparison tables; show codes in the top identity section.
- For comparison metrics, keep periods as columns and use one row per metric per fund.
- Compare standard performance metrics across YTD, 1Y, 3Y, 5Y, and all time.
- Keep beta versus the configured benchmark for now.
- Treat future asset-specific or fund-specific benchmark selection as out of scope.
- Replace top 30 holdings in comparison with **Common fund holding** rows.
- Fetch up to 200 holdings per fund for comparison, matching current single-fund analysis depth.
- Match common holdings by ticker when available on both sides; otherwise match by normalized holding name.
- Normalize holding names by trimming whitespace, case-folding, and collapsing repeated spaces.
- Do not implement fuzzy matching for common holdings.
- Include all reported holding types in common holdings.
- Sort common holdings by the larger of the two fund weights descending.
- Use fixed common-holdings columns: ticker, Fund A holding, Fund A weight, Fund B holding, Fund B weight.
- Use `—` for missing ticker values.
- Compare sector, country, and currency allocation side by side using the union of categories from both funds.
- Show missing allocation categories as `0,00%`.
- Sort allocation comparison rows by the larger fund weight descending.
- Use fixed allocation comparison columns: category, first fund weight, second fund weight.
- Use the selected comparison period only for fund-to-fund correlation and aligned return graph.
- Require full selected-period coverage for comparison correlation and graph; do not fall back to shorter overlapping history.
- Apply seven-day start and end tolerance for normal calendar and source cadence around graph/correlation coverage.
- Use today as the requested end date for fund-vs-fund comparison graph/correlation coverage.
- Compute fund-to-fund correlation from aligned daily log returns.
- Compute graph cumulative returns as price-relative returns from the first shared price in the selected period.
- Use an overlaid ASCII line chart for the aligned return graph, with a legend and start/end return summaries.
- Show the fund-to-fund correlation value above the graph.
- Put correlation and graph near the end of comparison output.
- `compare funds` updates holdings snapshot history for both funds but does not display snapshot diffs.
- Reuse the exact same snapshot key, fingerprint, tolerance, duplicate prevention, and diff display rules as single-fund analysis.
- Keep quote metadata out of snapshot fingerprints and holdings snapshot history.
- Add a `FundQuoteMetadata`-style model with optional name, AUM, AUM currency, inception date, and quote currency.
- Add a dedicated quote URL setting for the Morningstar quote endpoint.
- Keep quote endpoint query parameters private in the Morningstar source adapter.
- The verified quote endpoint works with the required query params and returns `400` without them.
- Preserve the ADR rule that Morningstar and Yahoo source details stay behind the market data Module and private source **Adapters**.
- Good deep module candidates are fund comparison calculation, common holdings matching, and quote metadata parsing/source access because each encapsulates several rules behind small testable interfaces.

## Testing Decisions

- Good tests should exercise external behavior through command parsing, service-level fund analysis/comparison interfaces, and market data source seams rather than private implementation details.
- Tests must not make network calls.
- Tests should use in-memory SQLite and fake market data sources, following existing testing conventions.
- Add CLI parsing tests for `analyze fund --code X --period 1y`.
- Add CLI parsing tests for `compare funds --code-a X --code-b Y --period 1y`.
- Add service tests for **Common fund holding** matching by ticker with different names.
- Add service tests for **Common fund holding** matching by normalized name when tickers are absent.
- Add service tests ensuring similar but unequal names do not match without ticker equality.
- Add service tests ensuring common holdings sort by the larger fund weight descending.
- Add service tests ensuring missing tickers can still match by normalized name and display as missing optional values in the result.
- Do not add dedicated tests for full-period coverage tolerance as part of this PRD unless implementation complexity makes them necessary.
- Do not add dedicated quote metadata parser tests as part of this PRD unless implementation complexity makes them necessary.
- Existing CLI command tests are prior art for command parsing behavior.
- Existing fund analysis tests are prior art for holdings aggregation, top-10 weight, and snapshot diff behavior.
- Existing correlation tests are prior art for correlation behavior and aligned return calculations.
- Existing market data tests are prior art for fake source injection and **Base currency** series behavior.

## Out of Scope

- Persisting quote metadata in SQLite is out of scope.
- Historical AUM tracking is out of scope.
- Replacing the existing holdings endpoint with the quote endpoint is out of scope.
- Changing NAV unitization rules is out of scope.
- Changing existing single-fund performance metric periods is out of scope.
- Changing beta to use fund-specific or asset-specific benchmarks is out of scope.
- Adding user-configurable benchmarks is out of scope.
- Adding fuzzy matching for common holdings is out of scope.
- Adding allocation weighting based on the user's actual portfolio allocation to the compared funds is out of scope.
- Adding fallback graph/correlation windows when full selected-period coverage is unavailable is out of scope.
- Adding a portfolio rebuild to `compare funds` is out of scope.
- Reworking existing snapshot storage schema is out of scope.
- Adding new database tables is out of scope unless implementation discovers an unavoidable need.
- Creating an ADR for the quote endpoint is out of scope; the decision is not hard to reverse enough.

## Further Notes

- The separate quote endpoint is `https://api-global.morningstar.com/sal-service/v1/fund/quote/v2/{code}/data`.
- The quote endpoint requires the existing SAL API key and user agent headers.
- The quote endpoint requires query params `locale=en`, `clientId=MDC`, `benchmarkId=mstarorcat`, and `version=4.71.0`.
- A manual endpoint check for `F00000WI0D` returned `200` with query params and `400` without them.
- The same manual check returned `investmentName`, `baseCurrencyId`, `tNAInShareClassCurrency`, and `inceptionDate`, which are sufficient for the requested quote metadata.
- `compare funds` having snapshot side effects is surprising for a comparison command, but it is an explicit user decision. The command should update holdings snapshot history without showing snapshot diffs. Future implementation may consider documenting the side effect in command help text.
- The glossary has been updated with **Fund candidate correlation**, **Common fund holding**, and **Fund quote metadata**.
