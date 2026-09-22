# Performance 01 baseline and decision gate

This is the baseline artifact for issue #20 / PRD #19 and the direct prepared-NAV
rollout gate for issue #68 / PRD #62. The executable harness
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
Rolling correlation now maintains bounded rolling statistics instead of
allocating and rescanning both 60-return windows for every output point. The
offline benchmark includes representative and stress end-to-end and direct
metric paths, and records a work/allocation proxy alongside timings without
network data. The allocation count is collected through the benchmark's
counting global allocator around the direct metric call.
The final issue #32 rerun also includes the full representative and stress NAV
rebuild paths.

Transaction listing is immaterial at small scale but grows substantially at
5,000 and 20,000 rows. Prepared NAV readiness trusts the latest Complete NAV
snapshot as its NAV rebuild checkpoint and does not audit earlier history.
Startup remains a separate measurable path.
The unindexed transaction plan shapes are evidence for later index work, but
their expected improvement is a hypothesis until that work is measured; no
bottleneck claim is inferred from a plan alone.

## Decision gate

The committed report is generated from actual Criterion estimate and sample
files by `generate-performance-baseline.sh`; no timing numbers are hand-authored.
The final full rerun timed out while collecting the stress NAV benchmark. The
report was therefore refreshed in results-only mode from the available
Criterion artifacts and records that provenance explicitly. It must not be
read as evidence that every path completed in one final run.
The issue #20 decision gate approved immutable p95 targets for the named paths;
the one exception is `nav_readiness_warm_representative`, whose original
`12,847,958 ns` target was explicitly user-approved as `20,338,526 ns` for the
current PR. Every other issue #20 target remains immutable. The generator
records this provenance under `decision_gate.target_provenance`, compares each
observed value to its fixed target, and records an explicit pass result under
`decision_gate.fixed_target_results`. Paths added by later issues remain
measured evidence only and have no fabricated acceptance target. The measured warm-cache source-call count is recorded separately from
the optimization target of zero; peak activity must be no greater than the
approved fixed limit.

“Warm” means the requested range has already been prepared. The final rerun
observed zero source calls for warm Historical market data preparation. Failed
source attempts remain retryable by later commands; no failure cooldown or
refresh policy was introduced by issue #32.

The concurrency candidates each run eight independent one-day Stock/EUR
preparations against separate file-backed SQLite fixtures. Every operation
makes one real delayed source call and one real cache write. The partial final
rerun did not retain candidate work output, so the generated report leaves that
section empty rather than inferring results. The previously approved production
limit of **4** remains in force.

## Prepared-NAV rollout evidence

The #68 gate adds two deterministic work proxies to the same offline Criterion
harness. `nav_plan_allocation_proxy` resets the benchmark's counting global
allocator after constructing the fixture, then records allocations for one
complete representative and stress rebuild. `nav_preparation_read_proxy` counts
only SQL `SELECT` statements during public NAV readiness for one and twenty
calendar years; persistence writes are intentionally excluded because generated
rows necessarily scale with history. The performance generator rejects missing
proxy output or unequal read counts, and stores both records in
`decision_gate.work_and_plan_output` without converting them into timing claims.

The benchmark stdout and Criterion artifacts are the provenance for every timing
value. If a full collection is interrupted, the report records the incomplete
collection and does not infer missing samples or pass status. Fixed targets remain
the approved issue #20 values, including their recorded provenance; a target
regression is a decision-gate failure, not a target change.

For the #68 direct rollout gate, two complete full runs failed the immutable
`transaction_listing`-family targets on code paths untouched by the rollout (and
one `rolling_metric_representative` run); every NAV-specific target and the
startup target passed in both. The exact run-to-run record and blocked
decision-gate status live in `docs/nav-rollout-performance-evidence.md`; the
committed `docs/performance-baseline-results.json` reflects the final full run
and records the concrete failing paths. Target changes or rejecting the
regression require explicit user approval, not a harness edit.

## Verification

Run `./generate-performance-baseline.sh`, then `cargo fmt`,
`cargo clippy --offline -- -D warnings`, and `cargo test --offline`. The generator runs Criterion,
the query-plan test, and rewrites the JSON report. No user database or network
is used by the harness.
