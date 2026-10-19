# Fund comparison against each fund's own reference — acceptance evidence

Issue #119 (parent #111). Branch `agent/benchmark-119-fund-comparison`, baseline
`3108d89d91b3db369be36eaa320a93ada872b58f`, pull request #136 against the
feature branch `agent/grill-benchmark-framework`.

## Verification

| Check | Result |
| --- | --- |
| `cargo fmt -- --check` | clean |
| `cargo clippy -- -D warnings` | clean |
| `cargo test` (offline) | 40 test binaries, 545 tests, 0 failed |
| `cargo clippy --test fund_comparison_reference_tests -- -D warnings` | clean |

No test performs a network call and no test opens the real portfolio
database. Every test uses in-memory SQLite, a fixed clock, and the fake
`MarketDataSources` seam.

## Review findings and how each one is answered

### The public identity writer sat below the private identity printer

`src/cli/display/fund_comparison.rs` now keeps the public
`write_fund_comparison_identity` above the private `print_identity`, as
`docs/CONVENTIONS.md` (Function Ordering) requires. The change is a move
only; no logic changed.

### The fund comparison was never exercised through the production dispatch

The JSON assertions used a test helper that called the service and wrote the
envelope by hand with a hardcoded `compare.funds` command name, so the
production command and the library dispatch were not covered. The correction
adds the real production path:

- `commands::compare::funds_with_writer` is the production command
  implementation. It writes the JSON envelope through the injected writer
  with the same `compare.funds` command identity the command always used, and
  `commands::compare::funds` keeps the existing stdout behaviour by locking
  stdout and delegating to it.
- The `Commands::Compare` branch of `src/cli/dispatch.rs` routes through that
  command, so the existing public `run_command_with_writer` now carries the
  fund comparison's envelope as well. The coordinator authorized this additive
  compare branch and its doc comment; the strategy, portfolio, adapter,
  migration, and root-CLI paths are untouched.
- Every JSON assertion in `tests/fund_comparison_reference_tests.rs` now
  comes from the library dispatch of a real `Commands::Compare` value, so the
  command identity and the document shape are the production ones and no test
  hand-builds an envelope.
- `the_compare_funds_command_dispatches_one_json_envelope` asserts that the
  dispatched command writes exactly one envelope (one line, one JSON value, no
  ANSI sequences beside it), that the command identity is the production
  `compare.funds`, that one side keeps its own reference identity while the
  other side's missing reference stays isolated, and that the human facts
  agree with the dispatched document.

**Recorded limitation:** the human renderer itself is not captured. It prints
the correlation graph with the chart library, which has no writer API, so the
human output keeps writing to the process stdout. The test therefore asserts
the human facts through the production human writers
(`write_fund_comparison_identity` and
`write_fund_comparison_reference_context`), which are the production
renderers of the identity and reference sections; it does not claim a
captured human stdout.

## Acceptance criteria and where each one is covered

### Existing direct fund-versus-fund analysis remains available alongside each fund's selected primary reference, aligned graph and standard-period relative results

- `src/services/fund_comparison.rs` keeps the direct analysis: the aligned
  correlation graph, the sector/country/currency allocations, the common
  holdings, and the standard fund-only performance rows.
- Each side's reference-relative comparison comes from the shared Benchmark
  Module (`src/services/benchmark.rs`): the selected primary reference, the
  aligned wealth series, and the five standard-period rows.
- `tests/fund_comparison_reference_tests.rs`:
  `each_fund_is_measured_against_its_own_reference`,
  `the_graph_period_does_not_redefine_the_standard_rows`.

### Exact class, reference identity, purpose, conventions, actual dates, and scoped limitations in human and JSON output

- `FundClassIdentity` (`src/models/fund_comparison.rs`) carries the source
  code, the tracked identity (the ISIN of a fund), and the vehicle;
  `write_fund_comparison_identity` renders it.
- `FundBenchmarkComparison.purpose` (`ReferencePurposeInfo`) carries the
  role, the universe, the instrument, the history boundary, the verification
  state, and the official and objective comparators. The identity and the
  return/currency/hedge conventions stay in the existing comparison fields.
- The scoped limitations stay in `fund_limitations` and
  `reference_limitations`, one scope per side.
- `tests/fund_comparison_reference_tests.rs`:
  `each_fund_is_measured_against_its_own_reference` (JSON and human
  assertions), `a_scoped_limitation_stays_in_its_own_scope`.

### An absent or unusable reference for one fund affects only dependent comparisons

- The comparison carries no shared reference any more: the top-level
  `benchmark_reference` (the legacy ACWI proxy) and the per-side
  `benchmark_limitations` are gone, and the human output has no shared
  "Beta reference" line.
- A side without a usable reference keeps its own outcome state and reason
  and never borrows the other side's reference.
- `tests/fund_comparison_reference_tests.rs`:
  `an_unusable_reference_affects_only_its_own_side`,
  `an_untracked_candidate_keeps_its_absence_visible`.

### Direct-comparison coverage rules, holdings snapshots, and graph-period independence

- The selected-period coverage rule of the direct correlation is unchanged
  (`coverage_reason` in `src/services/fund_comparison.rs`).
- The holdings snapshot of every compared fund is still recorded.
- The standard comparison rows are computed by the shared Benchmark Module
  from the comparison window, not from the selected graph period.
- `tests/fund_comparison_reference_tests.rs`:
  `the_direct_comparison_keeps_its_full_period_coverage_rule`,
  `the_holdings_snapshot_behavior_is_preserved`,
  `the_graph_period_does_not_redefine_the_standard_rows`.

### Candidate comparison creates no holding; classes and vehicles stay distinct; EUR never mixes an unconverted reference

- The comparison never writes an asset or a Transaction ledger row; it only
  records holdings snapshots.
- Each side keeps its own class identity, so two share classes of the same
  fund never read as one investment.
- The reference comparison is produced by the shared Benchmark Module, which
  converts the subject and the reference to EUR through the shared
  preparation and refuses an incompatible or unconverted convention.
- `tests/fund_comparison_reference_tests.rs`:
  `exact_classes_and_vehicles_stay_distinct_and_create_no_holding`,
  `a_native_currency_fund_is_never_compared_with_an_unconverted_reference`.

### Offline public fund-comparison and writer tests

`tests/fund_comparison_reference_tests.rs` holds eleven public tests: unlike
references, one-sided failure, class distinctions, an untracked candidate, the
full-period enforcement, one successful JSON envelope through the production
dispatch, and the shared preparation reuse (one ranged request per fund and
per reference).

## Decisions taken inside the acceptance criteria

- **The direct fund-to-fund rows are fund-only.** They are measured in each
  fund's own currency and carry no reference-dependent fact, so the Beta row
  measured against the old fixed ACWI proxy was removed from the direct
  table. The reference-relative beta, correlation, tracking error, and
  information ratio are reported per side by the shared Benchmark Module
  against that fund's own reference. This keeps the old fixed ACWI reference
  out of the comparison and stops two funds with different references from
  reading as one investment universe.
- **A fund that this portfolio does not track has no local class identity.**
  The comparison shows that absence and the reason ("no exact tracked-asset
  identity") instead of borrowing another class's identity. A candidate
  assignment (`fund_candidate` subject) still needs the exact ISIN of the
  compared class, which an untracked Morningstar code does not expose; that
  boundary is unchanged and is reported as an absent reference.
- **The direct comparison no longer reads the FX rate.** Its rows need no
  conversion, so a malformed persisted rate is not read by that path. The
  fund analysis and the reference comparison still read it, and a malformed
  persisted rate stays a hard error there
  (`malformed_persisted_fund_fx_is_a_hard_error_in_analysis_and_comparison`).
