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

Repeated bounded samples for every required path are emitted by the generator
into Criterion's `target/criterion` raw sample directories and stdout. The
benchmark covers cold, partial, and
warm preparation, cold and warm portfolio retrieval, full and incremental NAV,
listing at all three scales, both correlation paths, and startup. Source calls,
requested intervals, peak activity, and query-plan classification are recorded
alongside p50 and p95 distributions.

## Decision gate

The committed report is generated from actual Criterion estimate files by
`generate-performance-baseline.sh`; no timing numbers are hand-authored. The
decision gate derives path-specific targets as p95 plus 10 percent noise from
that report. Source work targets are zero successful warm-cache requests and
bounded peak activity; the fixed source-concurrency proposal is the smallest
candidate limit whose delayed-source p95 is within 10 percent of the best
candidate while preserving the lowest peak activity.

## Verification

Run offline with `cargo bench --bench performance -- --sample-size 10 --measurement-time 0.1`, then `cargo fmt`,
`cargo clippy -- -D warnings`, and `cargo test`. No user database or network is
used by the harness.
