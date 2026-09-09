# R77-D replay/readiness evidence

This report records the focused offline comparison requested for R77-D. The
benchmark is `benches/performance.rs`, run with the same host, Rust toolchain,
SQLite implementation, deterministic cached-source fixture, and Criterion
settings (`--sample-size 10 --measurement-time 0.05 --warm-up-time 0.05`).
Fixture construction is outside timed closures; no network source is used.

## Comparison

The table reports Criterion's measured `[low, point, high]` interval. Values are
wall-clock time per operation; `ms` and `s` are shown to keep the scale clear.

| path | input scale | b007d2f | f2b00d7 corrected base | 6d3722e patched head |
| --- | --- | ---: | ---: | ---: |
| `nav_readiness_warm_representative` | 50 assets / 10 years / 5,000 transactions | 416.05–426.26–439.54 ms | 407.38–415.37–423.59 ms | 407.07–417.97–429.30 ms |
| `nav_rebuild_incremental` | 5 assets / 1 year / 100 transactions; rebuild suffix from 2015-06-01 | 14.063–14.954–16.012 ms | 13.270–13.609–13.992 ms | 13.962–16.641–20.817 ms |
| `nav_rebuild_full` | 5 assets / 1 year / 100 transactions | 71.224–73.534–76.428 ms | 69.240–70.445–71.757 ms | 70.405–75.513–81.503 ms |
| `nav_rebuild_full_representative` | 50 assets / 10 years / 5,000 transactions | 5.1702–5.3013–5.4742 s | 4.9924–5.1104–5.2645 s | 5.3693–5.6011–5.8038 s |
| `nav_rebuild_full_stress` | 100 assets / 20 years / 20,000 transactions | 21.394–22.222–23.053 s | see note | 20.906–21.606–22.265 s |

The b007d2f and patched-head values are fresh same-machine runs. The
f2b00d7 stress run was started with the same harness but did not complete its
ten samples within the ten-minute execution window; it is intentionally not
represented by an invented number. The checked-in baseline artifact records a
prior f2b00d7-compatible stress estimate of mean 22.5236 s, median 21.6180 s,
and raw-sample p95 30.3948 s, but that historical result is not substituted for
the incomplete fresh comparison.

## Scope and decision

`CanonicalLedger::from_transactions` now sorts transaction references once,
converts them in canonical order, and passes typed entries directly to an
identity-validating constructor. This removes the transaction-list clone and
the second sort while preserving first malformed-entry ordering, `(date, id)`
identity, and all replay invariants. Portfolio current-position assembly
reuses the canonical replay produced during preparation rather than replaying
each open holding a second time; enrichment and market-data policy remain
separate from pure replay.

The warm-readiness path remains dominated by its complete-history audit, and
full rebuilds remain dominated by calendar-day and per-asset snapshot work.
The measured results do not justify a seeded replay interface, persisted read
models, sparse snapshots, skipped completeness audits, or moving market fetch
into replay. Cross-NAV/current-position replay sharing is deferred because a
warm readiness call has no replay payload and current-position enrichment needs
full transaction-date FX coverage; forcing that seam would add cloning or
change market-data boundaries without measured benefit.

Correctness remains covered by the existing full/seeded multi-date, same-day
ordering, sell/dividend reopening, FX-availability, and monetary-effect tests.
No benchmark timing is asserted by those tests.

## Reproduction

```text
cargo bench --offline --bench performance -- 'nav_' \
  --sample-size 10 --measurement-time 0.05 --warm-up-time 0.05
cargo test --offline --test performance_harness -- --nocapture
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline
```
