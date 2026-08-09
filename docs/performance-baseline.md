# Performance 01 baseline and decision gate

This is the baseline artifact for issue #20 / PRD #19.  The executable harness
is `benches/performance.rs`; it uses deterministic `XPERF###` identities, a
fixed clock, temporary file-backed SQLite, and an injected source. Fixture
construction is outside each Criterion timing closure, and no benchmark can
make a network request.

## Fixture matrix

| fixture | assets | period | transactions | purpose |
| --- | ---: | ---: | ---: | --- |
| small | 5 | 1 year | ~100 | correctness and iteration |
| representative | 50 | 10 years | ~5,000 | typical scaling |
| stress | 100 | 20 years | ~20,000 | upper-bound scaling |

The fixture generator intentionally uses multiple currencies and vehicle types;
the source series include calendar-day observations so forward-fill and stale
period cases can be measured deterministically.

## Measured versus hypotheses

Repeated bounded samples for every required path are recorded in
`performance-baseline-results.json`. The benchmark covers cold, partial, and
warm preparation, cold and warm portfolio retrieval, full and incremental NAV,
listing at all three scales, both correlation paths, and startup. Source calls,
requested intervals, peak activity, and query-plan classification are recorded
alongside p50 and p95 distributions.

## Decision gate

Measured targets are path-specific: warm preparation remains at zero source
calls and below 0.60 ms p95; partial preparation stays below 8.0 ms p95; full
small-fixture NAV stays below 50 ms p95; incremental NAV stays below 25 ms p95;
warm portfolio retrieval stays below 2.5 ms p95; representative listing stays
below 5 ms p95 and stress listing below 20 ms p95. These targets allow 10
percent noise over the observed p95. Delayed-source comparison shows p50 14.0
ms at limit 1, 8.1 ms at 2, 5.2 ms at 4, and 5.0 ms at 8. The proposed fixed
internal limit is therefore **4** because limit 8 adds no meaningful benefit.

## Verification

Run offline with `cargo bench --bench performance -- --sample-size 10 --measurement-time 0.1`, then `cargo fmt`,
`cargo clippy -- -D warnings`, and `cargo test`. No user database or network is
used by the harness.
