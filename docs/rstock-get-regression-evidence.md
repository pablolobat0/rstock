# `get` preparation regression and routine-output cleanup

## History

`b4ad77c8c42e80588da91aed692ae2a9c0950d58` introduced the preparation reuse
optimization on 16 August 2026. It merged through PR #44 on 21 August
(`1e4d88a`) and the performance rollout PR #46 on 23 August (`3ee28b8`).

The inventory-seam commit `6b5c84a` removed the readiness preparation flag and
scoped inventory preparation on 6 October. PR #97 (`6dbff54`, merged on
11 October) included that removal. Comparing #97 with its first parent shows
one lost runtime optimization: skipping repeated inventory historical
preparation after NAV prepared performance holdings. Single NAV readiness,
bounded position concurrency, and overlapping same-day price/FX requests
remain. The original delayed-source test was narrowed from the complete
portfolio operation to standalone inventory, losing coverage of this reuse.

## Production-path reproduction

On 12 October 2026 an offline diagnostic ran the production
`get_portfolio_view()` and human output Adapter against two isolated SQLite
backups of the application database. The before version was the committed
PR #97 tree; the after version included this fix. Both used a fixed clock of
12 October and injected sources returning no new observations. No network
requests were made, and the live database was opened read-only for backup.

| Observation | PR #97 | Fixed |
| --- | ---: | ---: |
| Asset interval source requests | 20 | 18 |
| Requests entirely before an asset's first transaction | 2 | 0 |
| Human baseline lines | 4 | 0 |
| Human mutual-fund exclusion lines | 9 | 0 |
| Human FRPH movement limitations | 1 | 0 |

Both unnecessary requests ended on 20 August 2025 for funds first bought on
2 December 2025. Inventory used another holding's 7 January start, with the
existing source padding extending the request into December 2024. The fixed
composer reuses NAV price preparation through command-scoped attempted-date
coverage in `MarketData`. Standalone inventory prepares each asset from its
own first transaction rather than a shared portfolio-wide start. Historical
transaction FX is independently prepared and consolidated by currency, even
when NAV resumes from a checkpoint.

These are source-call and output measurements, **not live-provider timing
measurements**. They demonstrate unnecessary requests but do not attribute
the whole reported 12-to-30-second increase. A missing series in a Morningstar
response also cannot be attributed to a specific interval from the original
warning-only logs.

## Automated regression coverage

`cargo test --test portfolio_performance_tests` exercises the production
services with in-memory SQLite and dummy source identities. Before the fix,
the staggered-fund dashboard and standalone-inventory cases requested
23 May through 5 June for a fund first bought on 6 June. Both assertions
failed. A separate checkpoint case fails when NAV preparation reuse is
disabled, even with per-asset preparation ranges fixed.

Two additional regressions exercise FX independently of price reuse: missing
pre-checkpoint acquisition FX must be recovered from the source, and multiple
holdings in one currency must make one consolidated historical FX request.
The initial blanket-reuse/per-asset-FX approach failed both; the final
MarketData-owned implementation passes both. The production database request
and output comparison was repeated against the final implementation with
the same fixed results above.

`weekday_without_quote_keeps_native_price_flat_and_includes_fx_movement` in
`tests/portfolio_inventory_tests.rs` now verifies that a missing same-day
quote retains the ordinary baseline/FX calculation without inventing an
actionable reporting-lag limitation. Genuine stale/missing-input limitations
from Individual price remain part of the movement outcome.

Human output omits routine mutual-fund exclusions and all baseline lines.
Eligible holdings excluded for missing price/FX still have concise partial
coverage output. Historical source failures with a cached price within the
existing acceptable-lag policy are diagnostic debug events; failures without
that fallback remain warnings.

Verification completed with `cargo fmt`, `cargo clippy -- -D warnings`, and
the complete `cargo test` suite passing.

Pre-commit standards and requested-behavior reviews passed after correcting
the historical-FX recovery and overlapping-request edge cases.
