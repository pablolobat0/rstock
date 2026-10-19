# Exposure-matched portfolio comparison — acceptance evidence

Issue #117 (parent #111). Branch `agent/benchmark-117-exposure-dashboard`,
baseline `0d86904322a63746bb27c2d5783da6741e9a6ad9`, pull request #134
against the feature branch `agent/grill-benchmark-framework`.

## Verification

| Check | Result |
| --- | --- |
| `cargo fmt -- --check` | clean |
| `cargo clippy -- -D warnings` | clean |
| `cargo test` (offline) | 38 test binaries, 491 tests, 0 failed |
| `cargo clippy --test exposure_benchmark_tests -- -D warnings` | clean |
| `cargo clippy --test exposure_benchmark_performance_tests -- -D warnings` | clean |

No test performs a network call and no test opens the real portfolio
database. The recorded CPU, memory, read, and source-attempt evidence is in
[exposure-benchmark-performance-evidence.md](exposure-benchmark-performance-evidence.md).

## Per-criterion evidence

### 1. Both dashboard aliases, library-owned dispatch, equivalent human/JSON outcomes

- `src/cli/dispatch.rs` adds `DashboardCommand::{Get, PortfolioGet}` and
  `run_dashboard_command_with_writer()`, which routes both aliases through
  `commands::portfolio::get_with_writer()`. `Commands::Get` and
  `Commands::Portfolio(PortfolioCommands::Get)` keep sharing
  `commands::portfolio::get()`.
- `src/cli/adapters/human.rs` renders the exposure section: the availability
  state, the comparison window and its anchor, the omitted seed interval, the
  retrospective note, the returns and the excess, every contributing
  reference with its full provenance, the standard rows, and the aligned
  subject/reference graph.
- `src/cli/adapters/json.rs` adds the `exposure_benchmark` key to the
  `portfolio.get` envelope; the availability state, its reason, and the
  per-fact states survive serialization.
- Tests: `both_dashboard_aliases_report_the_same_exposure_comparison`,
  `human_and_json_adapters_report_the_same_exposure_facts`.

### 2. Prior-close weights, complete NAV scope denominator, retained cash, Monetary exclusion, next-day trades

- `interval_scope()` divides the preceding close's per-asset EUR market
  values by the complete NAV scope and carries the retained Net dividend
  income as the 0%-return residual; `interval_reference_weights()` aggregates
  the weights per primary reference; the interval loop always reads the
  *previous* snapshot's exposures.
- Monetary holdings never enter the exposure history, because the NAV engine
  excludes them from performance valuation, and they are filtered
  defensively as well.
- Tests: `exposure_comparison_uses_prior_close_weights_and_anchors_both_series`
  (hand-calculated reference wealth 106.0, 110.24, 120.345333),
  `same_day_trade_changes_only_the_following_interval_weights` (the sold
  asset is still weighted 40% on its sale close and absent afterwards; the
  residual cash is 60/720 on that close),
  `retained_dividend_income_is_carried_at_zero_return`,
  `monetary_only_portfolio_retains_its_domain_outcomes`.

### 3. Closed positions, shared-reference aggregation, no renormalization, secondary exclusion

- `reference_contribution_windows()` prepares a reference only for the window
  it was held in, so a closed position contributes where held and its
  reference is not required afterwards.
- `interval_reference_weights()` aggregates the assets that share one
  reference into one contribution and observes every contributing reference in
  every interval, so a closed reference reports a 0% weight instead of its
  last held weight.
- Only the explicit `tracked` primary assignment is read; candidate and other
  subject types never enter the weights.
- A missing assignment makes the comparison unavailable, names the asset, and
  states that the weights are not renormalized.
- Tests: `assets_sharing_one_reference_aggregate_their_weights`,
  `secondary_and_candidate_assignments_never_enter_the_weights`,
  `missing_assignment_is_unavailable_and_never_renormalized`.

### 4. Anchoring, omitted seed interval, preserved NAV results, retrospective assignments

- The anchor is the first Complete NAV snapshot with a positive NAV scope,
  and both series start at 100 there. The zero-value seed snapshot before it
  is disclosed as the `OmittedSeedInterval`.
- The comparison never changes the NAV inception result or the Effective
  valuation date: the Portfolio view keeps its own performance outcomes, and
  the comparison reports its own window and inception date.
- Assignments are reported as retrospective with their assignment date; no
  effective-dated chain and no spliced series exist.
- Tests: `exposure_comparison_uses_prior_close_weights_and_anchors_both_series`,
  `comparison_discloses_the_omitted_seed_to_first_close_interval`,
  `standard_rows_report_the_comparison_anchors`.

### 5. Failure isolation and domain outcomes

- A failing reference source makes only the comparison unavailable; the
  inventory, the NAV readiness, and the portfolio-only performance outcomes
  stay intact.
- Empty and Monetary-only portfolios keep their domain outcomes and carry a
  not-applicable comparison.
- No exit/restart continuity is fabricated: the comparison window is the
  ready NAV history, and an undefined or invalid state stays an explicit
  state or an error.
- Tests: `benchmark_failure_leaves_the_portfolio_view_intact`,
  `empty_portfolio_retains_its_domain_outcomes`,
  `monetary_only_portfolio_retains_its_domain_outcomes`.

### 6. Reuse of the ready NAV and bounded ordered reads

- `portfolio_asset_history_repo::find_between()` reads the exposures of the
  comparison window in one ordered range query (date, then asset).
- Each contributing reference is prepared once through
  `market_data.selected_benchmark_reference_market_data()`, so the source
  attempts equal the number of contributing references.
- There is no ledger replay, no second NAV rebuild, no per-date query, and no
  per-date source request.
- Tests: `comparison_uses_bounded_reads_and_one_request_per_reference`,
  `exposure_comparison_work_does_not_scale_with_the_number_of_dates`,
  `exposure_comparison_performs_no_second_nav_rebuild`.

### 7. Offline fixtures, asserted states, and asserted anchors

- `tests/exposure_benchmark_tests.rs` holds 15 offline tests over in-memory
  SQLite, dummy assets, a fixed clock, and fake `MarketDataSources`. They
  cover prior-close weighting, next-day trade effects, retained cash, closed
  positions, shared references, first-close anchoring, missing assignments,
  failure isolation, empty and Monetary-only portfolios, the standard rows,
  and both dashboard aliases. The weights, returns, and actual anchors are
  asserted numerically, and both the human and the JSON states are asserted.

### 8. Recorded work evidence and the escalated cost concern

- `tests/exposure_benchmark_performance_tests.rs` and
  [exposure-benchmark-performance-evidence.md](exposure-benchmark-performance-evidence.md)
  record the representative and stress CPU, allocation, database-read, and
  source-attempt work on an already ready NAV history, so the incremental
  cost stays separate from the NAV rebuild, whose own evidence lives in
  [nav-rollout-performance-evidence.md](nav-rollout-performance-evidence.md).
- The stress fixture (50 holdings held for ten years) costs about 722 ms of
  CPU and about 59.5 MB of peak live bytes per comparison, of which about 70%
  is the bounded exposure read of 182,550 persisted rows. The cost is
  reported for the coordinator's decision; the calculation was not silently
  changed to monthly exposure weights and no millisecond or memory budget was
  invented.

## Contract decisions taken during the work

- The fund comparison keeps its accepted `fund_return_pct` standard-period
  row key, its public `FundBenchmarkPeriod` type, and the public
  `compute_standard_fund_benchmark_periods` signature. The shared
  calculation lives in `compute_standard_benchmark_periods()` and returns the
  neutral `BenchmarkPeriod` row; the fund comparison maps that row into its
  own output row, and the portfolio comparison exposes the neutral row
  directly. `fund_and_portfolio_rows_keep_their_own_keys_with_matching_facts`
  is the regression for both keys and the matching computed facts.
- No schema, migration, entity, or command surface was added.
  `benchmark_assignment_repo::find_by_subjects()` and the `assigned_at`
  field are additive repository changes, and
  `portfolio_asset_history_repo::find_between()` is the approved bounded
  ordered read seam.
