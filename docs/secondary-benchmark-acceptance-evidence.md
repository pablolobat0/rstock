# Optional secondary comparisons — acceptance evidence

Issue #120 (parent #111). Branch `agent/benchmark-120-secondary-comparisons`,
baseline `b94d752` (the integrated #118 Strategy and #119 fund-comparison
baseline), pull request against the feature branch
`agent/grill-benchmark-framework`.

## Verification

| Check | Result |
| --- | --- |
| `cargo fmt -- --check` | clean |
| `cargo clippy -- -D warnings` | clean |
| `cargo test --offline` | 43 test binaries, 596 tests, 0 failed |
| `cargo clippy --test secondary_benchmark_tests -- -D warnings` | clean |

No test performs a network call and no test opens the real portfolio
database. Every test uses in-memory SQLite, a fixed clock, and the fake
`MarketDataSources` seam.

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
  `benchmark remove-secondary-candidate`, dispatched through the existing
  library-owned dispatch and the injected writer. `benchmark list` reports
  the primary assignments and the secondary assignments separately, with the
  secondary role, the retrospective meaning, and the definition counts.
- **Shared Benchmark Module** — `compare_fund_to_secondary_references()`
  measures every explicitly selected secondary reference through the same
  preparation, the same convention gates, the same comparison calculation,
  and the same standard-period support as the primary comparison. The role
  distinguishes the two outcomes.
- **Output** — the fund analysis and the fund comparison report the
  secondary comparisons beside the primary one, in human and JSON, with the
  role, the reference identity, the conventions, the actual comparison dates,
  the standard rows, the scoped limitations, and the aligned wealth graph.

## Decisions taken inside the acceptance criteria

- **A secondary is stored apart from the primary, not as a role column.**
  One subject may hold several secondary references, so a separate table
  keeps the "one primary per subject" invariant of `benchmark_assignments`
  untouched and makes the separation structural instead of conventional.
- **The same reference cannot be the primary and the secondary of one
  subject.** One series compared with itself is not a second comparison, so
  the assignment is refused with a message that names the primary reference.
- **The secondary assignments of one exact class are read from both records
  of that class.** The Tracked asset and the Fund candidate of the same exact
  ISIN describe the same investment, so both records contribute their
  secondary selections and a reference selected by both is reported once.
  The primary resolution is unchanged: it still reads the Tracked subject
  only, as accepted in #119.
- **The unverified approved targets stay selectable and unusable.** The
  Cobas Europe Small Cap Value and the plain €STR definitions are approved
  target definitions with `target_only` verification, so they can be
  selected as secondary references but their comparison is an explicit
  unavailable outcome that names the unverified production mapping. No
  sector reference is seeded, and no unverified source becomes a usable
  comparison.
- **The largest dispatch branch is pinned on the heap.** The secondary
  comparison results grew the fund-comparison branch of
  `src/cli/dispatch.rs` past the strict `clippy::large_futures` threshold.
  The `CompareCommands::Funds` branch now pins its future with `Box::pin`,
  which keeps the dispatch future at 9816 bytes instead of 16496 bytes. The
  command, its output, its errors, and its JSON envelope are unchanged.

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
  the primary reference of a subject cannot be assigned as its secondary.
- `tests/secondary_benchmark_tests.rs`:
  `a_secondary_assignment_never_replaces_the_primary_and_keeps_the_candidate_identity`.

### Secondary weights are never included in the Exposure-matched portfolio benchmark. A missing secondary input cannot invalidate a complete primary comparison or portfolio-only facts

- The exposure-matched comparison reads only the primary assignments, and
  the comparison is byte-for-byte identical before and after a secondary
  assignment.
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

- The eleven public tests of `tests/secondary_benchmark_tests.rs` cover the
  assignment management, the separation, the independent availability, the
  exact candidate identity, the deletion protection, the absence of double
  counting, and the reuse of the shared standard-period support.
- `the_secondary_standard_rows_reuse_the_shared_period_support` compares the
  reported rows with `compute_standard_benchmark_periods()` over the same
  aligned series, so a second calculation would fail the test.
- `the_secondary_comparisons_of_one_exact_class_include_the_candidate_record`
  covers the exact-class resolution and the one-reference rule.

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
- The only change to an accepted test file is the migration-boundary lookup
  of `tests/strategy_tests.rs`: the destructive-rollback test now locates
  the Strategy selection migration by its stable name, exactly as
  `tests/custom_blend_tests.rs` already does, so an unrelated additive
  migration cannot shift the boundary the test exercises. The refusal and
  its message are unchanged.
- The new command leaves were added to the command-surface audit of
  `tests/cli_command_tests.rs`, as `docs/CONVENTIONS.md` requires.
