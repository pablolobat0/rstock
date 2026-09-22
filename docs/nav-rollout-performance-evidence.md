# Prepared NAV Direct-Rollout Evidence

This document is the evidence record for issue #68 / PRD #62. The prepared-plan
engine is the sole production NAV rebuild path; there is no feature flag, shadow
calculation, fallback engine, or routine full-history audit.

## Procedure

Run from the integration branch head with networking disabled:

```text
CARGO_NET_OFFLINE=true ./generate-performance-baseline.sh
```

The procedure builds dummy `XPERF###` fixtures in temporary file-backed SQLite,
uses a fixed benchmark clock and injected offline source observations, constructs
fixtures outside Criterion timing closures, and never opens the user database.
The small (5 assets / 1 year / 100 transactions), representative (50 / 10 years /
5,000), and stress (100 / 20 years / 20,000) fixtures cover full rebuild,
incremental rebuild, and warm readiness paths.

## Evidence Contract

The generated `docs/performance-baseline-results.json` is the numeric source of
truth. It records Criterion estimates and raw-sample p95 values, fixed-target
comparisons, warm Historical market-data source calls, and the following rollout
work evidence:

- `nav_plan_allocation_proxy`: allocation counts for complete representative and
  stress plans after fixture construction.
- `nav_preparation_read_proxy`: SQL read counts for one versus twenty calendar
  years at the public readiness seam; equal counts demonstrate no date-scaled
  preparation reads. Generated snapshot writes are not included in this proxy.
- `execution_database_reads`: public readiness tests assert zero database reads
  during prepared plan execution.

The performance generator rejects missing work evidence and rejects unequal
preparation-read counts. It does not invent timings, convert incomplete benchmark
collection into a pass, or alter immutable fixed targets. Any fixed-target
regression requires resolution or an explicit decision gate with provenance.

## Verification

## Decision gate record (BLOCKED)

Two complete offline runs of `./generate-performance-baseline.sh`
(`CARGO_NET_OFFLINE=true`, samples 10, warm-up 0.1 s, measurement-time 0.1 s,
release build) were collected on a shared 12-core Orca execution host. Both runs
recorded **fixed-target regressions** on `transaction_listing` paths, and run 1
also recorded one on `rolling_metric_representative`:

| Path | Run 1 p95 (ns) | Run 2 p95 (ns) | Immutable target (ns) |
| --- | --- | --- | --- |
| `transaction_listing` | 546,424.75 | 468,339.5 (passed) | 486,397 |
| `transaction_listing_representative` | 29,372,720 | 31,015,634 | 21,554,013 |
| `transaction_listing_stress` | 135,363,533 | 111,915,912 | 98,552,625 |
| `rolling_metric_representative` | 290,169.25 (failed) | 227,675.4 (passed) | 260,907 |

In both runs every NAV-specific target passed: `nav_rebuild_full`,
`nav_rebuild_full_representative`, `nav_rebuild_full_stress`,
`nav_rebuild_incremental`, and `nav_readiness_warm_representative`, as well as
`startup_and_migration`. Work evidence was captured in both runs: warm
Historical preparation made zero source calls, preparation `SELECT` counts were
equal across one and twenty calendar years (5 vs 5), and the deterministic
allocation proxy recorded complete representative and stress plan allocations.

Provenance and scope notes:

- The #68 change modifies only `benches/performance.rs`, the performance-harness
  test, `generate-performance-baseline.sh`, and documentation; it touches no
  transaction-listing, rolling-metric, or NAV source behavior. The failing
  targets exercise code paths unrelated to the prepared-NAV rollout.
- The failures vary between back-to-back runs on the same machine, consistent
  with shared-host load noise on micro-benchmarks rather than a code regression.
  This is an observation, not a proven root cause and not a basis for a pass.
- The listed targets are immutable user-approved issue #20 values; no target
  change was made and no rerun was discarded or cherry-picked: both recorded
- Because acceptance requires each fixed-target regression to be resolved or
  explicitly decided before rollout completion, the direct rollout is **BLOCKED
  at the decision gate**. Accepting these failures or changing the targets
  requires explicit user approval after PR review. Nothing in this document or
  in `docs/performance-baseline-results.json` claims the performance report
  verification or the rollout gate passed; the generated report records
  `decision_gate_status: "failed"` with the exact failing paths.

## Verification

```text
CARGO_NET_OFFLINE=true cargo fmt --check
CARGO_NET_OFFLINE=true cargo clippy --all-targets -- -D warnings
CARGO_NET_OFFLINE=true cargo test --workspace
```

The committed report must identify whether full Criterion collection completed;
historical R77 evidence in `docs/replay-performance-evidence.md` is not substituted
for #68 measurements.
