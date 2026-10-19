# Exposure-matched portfolio comparison — acceptance evidence

Issue #117 (parent #111). Branch `agent/benchmark-117-exposure-dashboard`,
baseline `0d86904322a63746bb27c2d5783da6741e9a6ad9`, pull request #134
against the feature branch `agent/grill-benchmark-framework`.

## Verification

| Check | Result |
| --- | --- |
| `cargo fmt -- --check` | clean |
| `cargo clippy -- -D warnings` | clean |
| `cargo test` (offline) | 38 test binaries, 495 tests, 0 failed |
| `cargo clippy --test exposure_benchmark_tests -- -D warnings` | clean |
| `cargo clippy --test exposure_benchmark_performance_tests -- -D warnings` | clean |

No test performs a network call and no test opens the real portfolio
database. The recorded CPU-independent work evidence — elapsed wall time,
allocation profile, read statements, and every source attempt — is in
[exposure-benchmark-performance-evidence.md](exposure-benchmark-performance-evidence.md).

## Review findings and how each one is answered

### 1. The Portfolio view swallowed every benchmark failure

The comparison service now separates the two outcomes itself:

- An **expected missing benchmark input** is an explicit `unavailable` state
  produced by the comparison service: a missing assignment, an unverified
  reference definition, an incompatible or unqualified source, a sparse
  cadence, incomplete EUR coverage, a missing qualified observation, and a
  persisted date without a per-asset exposure scope.
- A **database failure, a corrupt persisted observation, and a violated
  invariant** stay errors and propagate through `get_portfolio_view()` and the
  command dispatch. The blanket `Err => unavailable` catch in
  `src/services/portfolio_view.rs` is gone.

Regressions: `a_failed_database_read_is_a_hard_error` (the exposure table is
dropped), `a_malformed_persisted_reference_price_is_a_hard_error` (a
malformed persisted reference price), and
`a_mismatched_exposure_total_is_a_hard_error` (the persisted exposures do
not sum to the persisted asset value). The expected absences keep the
inventory, the ready NAV history, and the portfolio-only outcomes:
`benchmark_failure_leaves_the_portfolio_view_intact`,
`a_missing_fx_rate_keeps_the_portfolio_only_outcomes`,
`missing_assignment_is_unavailable_and_never_renormalized`, and
`a_missing_internal_weekday_observation_makes_the_comparison_unavailable`.

### 2. The forward-filled reference series was used as if it were observations

The comparison now uses the source-reported observations only. A qualified
observation must exist on the comparison anchor, on the comparison end, and on
every completed weekday of each contributing reference's window. A forward
filled row is not an independent mark, so it never stands in for a missing
one, and the comparison never falls back to another reference. The
observation dates are the anchor and the completed weekdays, so both compared
series share one cadence.

Regression: `a_missing_internal_weekday_observation_makes_the_comparison_unavailable`
— the reference stops reporting on 2025-01-09 and the comparison becomes
unavailable with the missing close named.

### 3. The structured limitations and the reference provenance were discarded

Every outcome now carries the selected references with their identity, source,
source identifier, return basis, adjustment, hedge policy, currency, cadence,
verification state, assignment date, and their Market data limitations, on the
available **and** the unavailable paths. The limitations keep the classified
subject (asset, benchmark reference, or FX rate) and their requested and
latest available dates.

Regressions: `benchmark_failure_leaves_the_portfolio_view_intact`,
`missing_assignment_is_unavailable_and_never_renormalized`,
`a_missing_fx_rate_keeps_the_portfolio_only_outcomes`, and
`human_and_json_adapters_report_the_same_unavailable_facts`.

### 4. The JSON Adapter serialized the domain outcome directly

`src/cli/adapters/json.rs` owns the exposure benchmark schema through explicit
Adapter DTOs — the comparison, the references, the omitted seed interval, the
wealth points, the standard periods, the availability, the calculated facts,
and the Market data limitations. The domain outcome no longer derives
`Serialize` for this path, so no domain serde representation leaks into the
JSON contract. The accepted fund JSON key, type, and signature are unchanged.

### 5. The writer-injected dispatch and the public calculation seam were not production paths

- `src/cli/dispatch.rs` now has `run_command_with_writer()`, and `run_command()`
  delegates to it. The dashboard aliases are dispatched through that one
  production path, and the tests capture the output of the real
  `Commands::Get` and `Commands::Portfolio(PortfolioCommands::Get)` commands.
  The alternate `DashboardCommand` router is gone.
- The public test-only calculation helper (`compute_interval_exposure`,
  `AssetExposureWeight`, `IntervalExposure`) is removed. The tests assert the
  prior-close weights through the production outcome instead.

Regression: `both_dashboard_aliases_report_the_same_exposure_comparison`
dispatches both aliases through `run_command_with_writer`.

### 6. The performance evidence was not trustworthy

- The measurement is isolated: the performance file holds exactly one test, so
  no other test of the process allocates inside the measured window, and every
  fixture is built before the allocator window opens.
- The timings are labelled **elapsed wall time**, not CPU time.
- The bounded read is a separate measurement of that read. It is not
  subtracted from the comparison timing, and the earlier "comparison CPU
  without the bounded read" claim is withdrawn.
- Every source attempt is counted, including the benchmark predecessor
  lookups; `tests/common/mod.rs` records them and the counted total is
  asserted.
- The recorded representative and stress numbers were recollected; the
  previous 722 ms CPU and 59 MB figures are not trusted and are replaced by
  the release-build elapsed wall time and the corrected allocation profile.

### 7. The assignment timestamp was printed as a raw machine value

The human Adapter prints the assignment date in the display date format
(`11-01-2025`), while the JSON keeps the stored ISO value.

## Per-criterion evidence

### 1. Both dashboard aliases, library-owned dispatch, equivalent human/JSON outcomes

- `src/cli/dispatch.rs`: `run_command_with_writer()` is the production
  dispatch; `Commands::Get` and `Commands::Portfolio(PortfolioCommands::Get)`
  both render through `commands::portfolio::get_with_writer()`.
- `src/cli/adapters/human.rs`: the exposure section renders the availability
  state, the comparison window and its anchor, the omitted seed interval, the
  retrospective note, the returns and the excess, every contributing
  reference with its full provenance and limitations, the standard rows, and
  the aligned subject/reference graph.
- `src/cli/adapters/json.rs`: the `exposure_benchmark` key of the
  `portfolio.get` envelope is an explicit Adapter mapping; the availability
  state, its reason, and the per-fact states survive serialization.
- Tests: `both_dashboard_aliases_report_the_same_exposure_comparison`,
  `human_and_json_adapters_report_the_same_exposure_facts`,
  `human_and_json_adapters_report_the_same_unavailable_facts`.

### 2. Prior-close weights, complete NAV scope denominator, retained cash, Monetary exclusion, next-day trades

- `interval_scope()` divides the preceding close's per-asset EUR market
  values by the complete NAV scope and carries the retained Net dividend
  income as the 0%-return residual; `interval_reference_weights()` aggregates
  the weights per primary reference; every interval starts at the previous
  observed close, so a same-day trade changes the next interval's weights
  only.
- Monetary holdings never enter the exposure history, because the NAV engine
  excludes them from performance valuation, and they are filtered
  defensively as well.
- Tests: `exposure_comparison_uses_prior_close_weights_and_anchors_both_series`
  (hand-calculated reference wealth 106.0, 110.24, 120.345333),
  `same_day_trade_changes_only_the_following_interval_weights`,
  `retained_dividend_income_is_carried_at_zero_return`,
  `monetary_only_portfolio_retains_its_domain_outcomes`.

### 3. Closed positions, shared-reference aggregation, no renormalization, secondary exclusion

- A reference is prepared only for the window it was held in, so a closed
  position contributes where held and its reference is not required
  afterwards.
- The assets that share one reference aggregate into one contribution, and
  every contributing reference is observed in every interval, so a closed
  reference reports a 0% weight instead of its last held weight.
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
- A database failure, a corrupt persisted observation, and a violated
  invariant remain errors.
- No exit/restart continuity is fabricated: the comparison window is the
  ready NAV history, and an undefined or invalid state stays an explicit
  state or an error.
- Tests: `benchmark_failure_leaves_the_portfolio_view_intact`,
  `empty_portfolio_retains_its_domain_outcomes`,
  `monetary_only_portfolio_retains_its_domain_outcomes`,
  `a_failed_database_read_is_a_hard_error`,
  `a_malformed_persisted_reference_price_is_a_hard_error`,
  `a_mismatched_exposure_total_is_a_hard_error`.

### 6. Reuse of the ready NAV and bounded ordered reads

- `portfolio_asset_history_repo::find_between()` reads the exposures of the
  comparison window in one ordered range query (date, then asset).
- Each contributing reference is prepared once through
  `market_data.selected_benchmark_reference_market_data()`, so the source
  attempts are two per reference: one history request and one predecessor
  lookup.
- There is no ledger replay, no second NAV rebuild, no per-date query, and no
  per-date source request.
- Tests: `comparison_uses_bounded_reads_and_one_request_per_reference`,
  `the_comparison_performs_no_second_nav_rebuild`,
  `exposure_comparison_work_does_not_scale_with_the_number_of_dates`.

### 7. Offline fixtures, asserted states, and asserted anchors

- `tests/exposure_benchmark_tests.rs` holds 22 offline tests over an in-memory
  database, dummy assets, a fixed clock, and fake `MarketDataSources`. They
  cover prior-close weighting, next-day trade effects, retained cash, closed
  positions, shared references, first-close anchoring, missing assignments,
  missing observations, the hard-error paths, failure isolation, empty and
  Monetary-only portfolios, the standard rows, and both dashboard aliases.
  The weights, returns, and actual anchors are asserted numerically, and both
  the human and the JSON states are asserted.

### 8. Recorded work evidence and the escalated cost concern

- `tests/exposure_benchmark_performance_tests.rs` and
  [exposure-benchmark-performance-evidence.md](exposure-benchmark-performance-evidence.md)
  record the representative and stress work evidence on an already ready NAV
  history, so the incremental cost stays separate from the NAV rebuild, whose
  own evidence lives in
  [nav-rollout-performance-evidence.md](nav-rollout-performance-evidence.md).
- The stress fixture (50 holdings held for ten years) costs about 741 ms of
  elapsed wall time and about 59.5 MB of peak live bytes per comparison. The
  cost is reported for the coordinator's decision; the calculation was not
  silently changed to monthly exposure weights and no millisecond or memory
  budget was invented.

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
  `benchmark_assignment_repo::find_by_subjects()` and the
  `PrimaryAssignmentRecord` type are additive repository changes that keep
  the shared `PrimaryAssignment` catalog view unchanged, and
  `portfolio_asset_history_repo::find_between()` is the approved bounded
  ordered read seam.
