# Optional secondary comparisons — acceptance evidence

Issue #120 (parent #111). Branch `agent/benchmark-120-secondary-comparisons`,
baseline `b94d752` (the integrated #118 Strategy and #119 fund-comparison
baseline), pull request #139 against the feature branch
`agent/grill-benchmark-framework`.

## Verification

| Check | Result |
| --- | --- |
| `cargo fmt -- --check` | clean |
| `cargo clippy --offline -- -D warnings` | clean |
| `cargo test --offline` | 43 test binaries, 600 tests, 0 failed |
| `cargo clippy --test secondary_benchmark_tests -- -D warnings` | clean |

The recorded exit codes and the complete logs of these four commands are in
`docs/secondary-benchmark-verification.log`. No test performs a network call
and no test opens the real portfolio database. Every test uses in-memory
SQLite, a fixed clock, and the fake `MarketDataSources` seam.

## What was built

- **Persistence** — a new `benchmark_secondary_assignments` table
  (`subject_type`, `subject_id`, `reference_id`, `assigned_at`) with a
  foreign key on `benchmark_references.id`. It is stored apart from
  `benchmark_assignments`, so a secondary can never replace a primary
  reference, one subject can hold several secondaries, and the
  exposure-matched portfolio benchmark — which reads only the primary
  assignments — can never pick up a secondary weight. Rolling the migration
  back is refused while secondary assignments exist.
- **Public commands** — `benchmark assign-secondary`,
  `benchmark remove-secondary`, `benchmark assign-secondary-candidate`, and
  `benchmark remove-secondary-candidate`, dispatched through the library-owned
  dispatch and the injected writer. `benchmark list` reports the primary
  assignments and the secondary assignments separately, with the secondary
  role, the retrospective meaning, and the definition counts.
- **Exact class identity** — a Tracked asset and the Fund candidate of one
  exact ISIN and vehicle describe the same investment. The primary reference
  and the secondary comparison of one exact class must be different
  comparisons, in both assignment directions, and a rejected selection leaves
  no assignment behind.
- **Untracked candidate classes** — `analyze fund --candidate
  <vehicle>:<ISIN>` and `compare funds --candidate-a/--candidate-b
  <vehicle>:<ISIN>` state the exact class of an analysed fund with its
  verified vehicle and ISIN. The secondary comparisons of that class are then
  reported without a Tracked asset, while the primary comparison keeps the
  accepted #119 contract of an absent reference for an untracked class.
- **Shared Benchmark Module** — `compare_fund_to_secondary_references()`
  measures every explicitly selected secondary reference through the same
  preparation, the same convention gates, the same comparison calculation,
  and the same standard-period support as the primary comparison. The role
  distinguishes the two outcomes.
- **Output** — the fund analysis and the fund comparison report the
  secondary comparisons beside the primary one, in human and JSON, with the
  role, the reference identity, the conventions, the actual comparison dates,
  the standard rows, the scoped limitations, and the aligned wealth graph.

## Review findings and how each one is answered

### P2 — the primary/secondary collision check only covered the identical storage subject

`src/services/benchmark_references.rs` now resolves the *exact class* of a
subject before every selection. `exact_class_subjects()` returns the records
that describe one investment: the Tracked asset and the Fund candidate of one
exact ISIN and vehicle. A candidate `fund:LU1598719752` and the Tracked asset
`LU1598719752` are one exact class, so a selection of one reference for one
of them is a selection for that class.

- `save_secondary_assignment()` rejects a reference that is already the
  primary reference of any record of the exact class.
- `save_assignment()` rejects a reference that is already a secondary
  comparison of any record of the exact class.
- Both checks run before the write, so a rejected selection is atomic: no
  assignment is created, replaced, or removed.

The finding named two accepted behaviours, `candidate fund:ISIN secondary R
while tracked ISIN primary R` and `assign secondary R then existing public
primary assign R`. Both are now rejected with a message that names the
conflicting role, and the exact-class union can no longer render one series
as both the primary and the secondary comparison.

Evidence: `tests/secondary_benchmark_tests.rs`:
`the_primary_and_the_secondary_of_one_exact_class_cannot_share_a_reference`
(which also asserts that the rejected selection leaves no assignment behind
and that the primary reference stays in place).

### P2 — the secondary comparison returned nothing without a Tracked asset

`compare_fund_to_secondary_references()` now resolves the secondary
assignments of the exact class of the analysed fund from every record of that
class, including the Fund candidate record of an untracked class. The
identity of an untracked class comes from the user, who states the verified
vehicle and ISIN of the analysed fund:

- `rstock analyze fund --code <morningstar code> --candidate fund:LU1598719752`
- `rstock compare funds --code-a <code> --candidate-a fund:LU1598719752 ...`

The candidate record is never matched by name, no Tracked asset row is
created, and the primary comparison of an untracked class keeps the accepted
#119 contract (an absent reference with the reason "no exact tracked-asset
identity"). An explicit identity that contradicts the tracked class of the
same fund is rejected instead of mixing two classes in one comparison.

Evidence: `tests/secondary_benchmark_tests.rs`:
`an_untracked_candidate_reports_its_secondary_comparison_without_a_tracked_asset`,
`the_fund_comparison_reports_the_secondary_comparison_of_an_untracked_candidate`.
Both fixtures hold no `assets` row for the analysed fund, and both assert the
aligned returns, the standard rows, and the available state of the secondary
comparison. `an_explicit_candidate_identity_must_denote_the_analysed_class`
covers the identity conflict.

### P2 — the tests called handlers and serialized the analysis by hand

Every command in `tests/secondary_benchmark_tests.rs` now travels through
`cli::run_command_with_writer()` with a real parsed `Commands` value:

- The benchmark management commands write their successful output through the
  injected writer, so `src/cli/dispatch.rs` routes them through
  `commands::benchmarks::run_with_writer()`. The command adapter's stdout
  behaviour is unchanged, because the production dispatch passes the locked
  stdout as the writer.
- `commands::analyze::fund_with_writer()` writes the `analyze.fund` envelope
  through the injected writer, and the dispatch routes the analysis through
  it. The human renderer still prints its chart to the process stdout, so the
  human facts are asserted through the production human renderer
  (`write_fund_analysis_reference_context`), exactly as the accepted #119
  test does for the fund comparison.
- Each test asserts the command identity of the produced envelope
  (`benchmark.assign_secondary`, `benchmark.list`, `analyze.fund`,
  `compare.funds`), the JSON availability states, and the human facts. No
  test hand-builds an envelope, and no renderer queries data.

The accepted #119 compare-funds writer is unchanged: the comparison still
writes one JSON envelope through the injected writer.

### P3 — the private helpers of the shared Benchmark Module preceded the public functions

`src/services/benchmark.rs` now keeps the public entry points
(`compare_fund_to_primary`, `compare_fund_to_secondary_references`,
`compare_aligned_eur_series`, `compute_standard_fund_benchmark_periods`,
`compute_standard_benchmark_periods`) above the private helpers, as
`docs/CONVENTIONS.md` (Function Ordering) requires. The private
`secondary_assignments_of_exact_class()` helper and the subject label helper
now sit below them.

### P3 — the migration read its assignment count as an absent count

`migration/src/m20261020_000001_add_secondary_benchmark_assignments.rs` now
reads the assignment count as an invariant: a row that cannot be read, or a
count that cannot be decoded, is an error that refuses the destructive
rollback. The previous `unwrap_or_default()` chain turned a decode failure
into an empty count, which would have dropped the table and the assignments
with it.

Evidence: `tests/secondary_benchmark_tests.rs`:
`the_secondary_assignment_migration_protects_the_assignments_and_the_history`
(refuses the in-memory rollback while a secondary assignment exists, keeps the
assignment, the ledger, and the NAV history, and rolls back cleanly with the
history intact once the assignment is removed).

### Heuristic — role and vehicle strings

The comparison role is now a closed vocabulary: `models::ComparisonRole`
(`Primary`, `Secondary`) replaces the free role strings in the comparison
outcome and in the assignment outcome, and it keeps the accepted JSON values
`primary` and `secondary`. The candidate vehicle reuses the existing
`FundCandidateVehicle` enum, and the tracked asset type is read as the
existing `AssetType` enum, so the exact-class rule no longer compares free
strings.

## The confirmed bugs, reproduced before the fix

Each confirmed bug was reproduced against the pre-fix code, with the new
public tests failing, and the same tests passing after the fix:

| Finding | Reproduction (pre-fix) | Result |
| --- | --- | --- |
| P2 collision check | the candidate secondary of the exact class's primary reference was accepted | `the_primary_and_the_secondary_of_one_exact_class_cannot_share_a_reference` failed |
| P2 untracked candidate | the secondary comparison list was empty without a Tracked asset | `an_untracked_candidate_reports_its_secondary_comparison_without_a_tracked_asset` and `the_fund_comparison_reports_the_secondary_comparison_of_an_untracked_candidate` failed |
| P2 command routing | the benchmark command wrote to the process stdout, not to the injected writer | `secondary_assignments_are_managed_through_the_public_commands` failed |

## Acceptance criteria and where each one is covered

### Public commands manage explicit secondary assignments for assets/candidates; display aligned secondary returns/graphs using the shared Benchmark Module with role, convention and date provenance in human/JSON output

- `src/cli/mod.rs` declares the four secondary leaves; `src/cli/commands/benchmarks.rs`
  renders them through the injected writer, in one JSON envelope or one
  human text.
- `src/services/benchmark_references.rs` owns the assignment semantics:
  `assign_secondary_tracked_asset`, `assign_secondary_fund_candidate`,
  `remove_secondary_tracked_asset`, and `remove_secondary_fund_candidate`.
- `src/services/benchmark.rs` measures the secondaries through the shared
  preparation, gates, calculation, and standard-period support.
- `tests/secondary_benchmark_tests.rs`:
  `secondary_assignments_are_managed_through_the_public_commands`,
  `a_secondary_comparison_is_measured_beside_the_primary_comparison`,
  `the_fund_comparison_reports_the_secondary_comparisons_of_each_side`.

### Support the approved Cobas Europe Small Cap Value and DNCA plain-€STR secondary targets without treating unverified sources as usable. Sector references require research and explicit confirmation rather than automatic seeding

- Both definitions are selected explicitly and keep their `target_only`
  verification state, so their comparison is unavailable with the reason
  that names the unverified production mapping.
- No sector reference is created, and no assignment is applied without an
  explicit command.
- `tests/secondary_benchmark_tests.rs`:
  `the_approved_secondary_targets_are_selected_but_never_used_as_unverified_series`.

### Preserve Official fund comparator metadata separately; secondary selection does not replace the primary unless explicitly requested

- The candidate comparator provenance stays in `benchmark_candidates` and in
  the catalog's `candidates` section; the secondary assignment never writes
  it and never replaces it.
- The primary assignment row is never touched by a secondary command, and
  the primary reference of an exact class cannot be assigned as its
  secondary, nor a secondary of an exact class as its primary.
- `tests/secondary_benchmark_tests.rs`:
  `a_secondary_assignment_never_replaces_the_primary_and_keeps_the_candidate_identity`,
  `the_primary_and_the_secondary_of_one_exact_class_cannot_share_a_reference`.

### Secondary weights are never included in the Exposure-matched portfolio benchmark. A missing secondary input cannot invalidate a complete primary comparison or portfolio-only facts

- The exposure-matched comparison reads only the primary assignments, and
  the comparison is identical before and after a secondary assignment.
- A secondary whose input is missing leaves only that secondary unavailable;
  the primary comparison and the fund-only rows keep their own outcomes.
- `tests/secondary_benchmark_tests.rs`:
  `a_secondary_assignment_never_enters_the_exposure_matched_weights`,
  `a_missing_secondary_input_leaves_the_primary_comparison_intact`.

### Definition deletion checks secondary assignments too. Reassignment remains explicitly retrospective without historical splices

- `src/services/custom_blends.rs` refuses to delete a definition that a
  secondary assignment still uses, in the same caller-owned transaction as
  the usage check, and the refusal names the secondary assignment.
- A replacement selects the new reference for the whole window: the
  comparison opens at the first observation and closes at the end, with no
  effective-dated chain and no spliced series.
- `tests/secondary_benchmark_tests.rs`:
  `a_definition_used_by_a_secondary_assignment_cannot_be_deleted`,
  `a_secondary_replacement_compares_the_whole_window_without_a_splice`.

### Offline assignment, analysis and output tests cover primary/secondary separation, independent availability, candidate identity, deletion protection and no double counting. Additional relative metrics reuse standard-period support when present rather than duplicating it

- The fifteen public tests of `tests/secondary_benchmark_tests.rs` cover the
  assignment management, the separation, the independent availability, the
  exact candidate identity, the deletion protection, the absence of double
  counting, and the reuse of the shared standard-period support.
- `the_secondary_standard_rows_reuse_the_shared_period_support` compares the
  reported rows with `compute_standard_benchmark_periods()` over the same
  aligned series, so a second calculation would fail the test.
- `an_explicit_candidate_identity_must_denote_the_analysed_class` covers the
  identity conflict: an identity that names another class is rejected instead
  of mixing two classes in one comparison.

## Boundaries that were not crossed

- No production reference was qualified, no sector mapping was invented, and
  no production €STR or equity access is required by the tests.
- The real portfolio database was never opened, no network call was made, and
  no production data source was enabled.
- The accepted #118 Strategy contracts and the accepted #119 fund-comparison
  contracts are unchanged: the fund JSON keeps its `fund_return_pct` field
  and its `FundBenchmarkPeriod` rows, the strategy selection and its
  dashboard section are untouched, and the fund comparison still writes one
  JSON envelope through the injected writer.
- The accepted test files were touched only where the new command options
  require it: `tests/fund_analysis_tests.rs` and
  `tests/fund_comparison_reference_tests.rs` pass the new optional candidate
  options as `None`, which keeps every accepted assertion unchanged, and
  `tests/strategy_tests.rs` locates the Strategy selection migration by its
  stable name, exactly as `tests/custom_blend_tests.rs` already does, so an
  unrelated additive migration cannot shift the boundary the test exercises.
- The new command leaves were added to the command-surface audit of
  `tests/cli_command_tests.rs`, as `docs/CONVENTIONS.md` requires.
