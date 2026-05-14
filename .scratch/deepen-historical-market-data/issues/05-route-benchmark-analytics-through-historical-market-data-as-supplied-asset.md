# Route Benchmark Analytics Through Historical Market Data as Supplied Asset

Status: needs-triage

Type: AFK

## Parent

`.scratch/deepen-historical-market-data/PRD.md`

## What to build

Move benchmark asset lookup or creation out of Historical market data and into analytics setup. Benchmark analytics should then pass the benchmark asset through the same Historical market data preparation path used for other required assets, while benchmark data remains distinct from holdings.

## Acceptance criteria

- [ ] Analytics owns benchmark asset lookup or creation before Historical market data preparation.
- [ ] Historical market data prepares benchmark asset prices through the same supplied-asset path used for other assets.
- [ ] Benchmark FX is inferred from the benchmark asset currency.
- [ ] Benchmark data remains distinct from holdings and does not create portfolio snapshots.
- [ ] Benchmark analytics preserves existing Historical market data availability behaviour.
- [ ] Tests cover benchmark preparation through the supplied-asset path, inferred benchmark FX, and benchmark/holding separation.

## Blocked by

- `.scratch/deepen-historical-market-data/issues/02-infer-fx-inside-historical-market-data-preparation.md`
