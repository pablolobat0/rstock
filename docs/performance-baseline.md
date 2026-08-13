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
the source series include deterministic interior gaps so forward-fill and retry
work can be measured without network access.

## Measured versus hypotheses

`docs/performance-baseline-results.json` is the numeric source of truth and
contains the generated Criterion mean, median, standard deviation, and
normalized raw-sample p95 for every path. The measured dominant paths are the
representative correlation matrix, portfolio retrieval, and Historical market
data preparation. Small-fixture cold portfolio retrieval and full NAV rebuild
are also material; incremental NAV and representative rolling correlation form
the next tier.

Transaction listing is immaterial at small scale but grows substantially at
5,000 and 20,000 rows. Already-warm representative NAV readiness is
comparatively immaterial, while startup remains a separate measurable path.
The unindexed transaction plan shapes are evidence for later index work, but
their expected improvement is a hypothesis until that work is measured; no
bottleneck claim is inferred from a plan alone.

## Decision gate

The committed report is generated from actual Criterion estimate and sample
files by `generate-performance-baseline.sh`; no timing numbers are hand-authored.
Each named path's target is its generated p95 plus a 10 percent noise allowance,
recorded under `decision_gate.path_p95_targets_ns`. The measured warm-cache
source-call count is recorded separately from the optimization target of zero;
peak activity must be no greater than the approved fixed limit.

“Warm” means the requested range has already been prepared. The baseline still
observes six source retries because interior source gaps are not cached as
successful Historical market data. Eliminating that redundant source work is a
later optimization target, not a claim about the current implementation.

The concurrency candidates each run eight independent one-day Stock/EUR
preparations against separate file-backed SQLite fixtures. Every operation
makes one real delayed source call and one real cache write. Calls were equal at
eight and observed peaks were 1, 2, 4, and 8 for limits 1, 2, 4, and 8. The
generated p95 values improve through limit 4; limit 8 has a lower mean but
a higher p95 because of an observed outlier. The fixed source-concurrency
proposal is therefore **4**, the smallest candidate within 10 percent of the
best candidate p95 (and the best candidate itself). This proposal and the
generated path targets remain pending the decision gate below.

## Verification

Run `./generate-performance-baseline.sh`, then `cargo fmt`,
`cargo clippy -- -D warnings`, and `cargo test`. The generator runs Criterion,
the query-plan test, and rewrites the JSON report. No user database or network
is used by the harness.
