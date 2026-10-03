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

- `nav_plan_allocation_proxy`: allocation-call counts for complete
  representative and stress plans after fixture construction. These are
  allocation-work evidence (how much allocation work one complete rebuild
  performs), not memory measurements.
- `nav_plan_memory_proxy`: the byte-aware allocator profile of one complete
  end-to-end rebuild on the stress fixture, collected in a dedicated
  measurement window: scoped allocation calls, allocated bytes, deallocated
  bytes, and the running balance's peak and final live bytes.
- `nav_preparation_read_proxy`: SQL read counts for one versus twenty calendar
  years at the public readiness seam; equal counts demonstrate no date-scaled
  preparation reads. Generated snapshot writes are not included in this proxy.
- Zero database reads during prepared plan execution: asserted by the public
  readiness tests via the `execution_database_reads` probe on the execution
  sink; it is not a generated report field.

`nav_plan_memory_proxy` measurement scope and exclusions, stated precisely:

- Measured process memory: bytes routed through the Rust `GlobalAlloc`
  implementation on the benchmark's counting allocator (system allocator
  wrapper). It is an allocation-level high-water/live profile, not an RSS or
  page-level end-to-end memory measurement.
- Measured window: from opening the window (after the 100-asset, 20-year,
  20,000-transaction stress fixture is fully constructed) until the public
  `ensure_portfolio_history` readiness call completes; this spans preparation,
  execution, and atomic snapshot persistence of one rebuild. Fixture
  construction itself is outside the window.
- Included: every Rust-allocated byte on any thread of the benchmark process
  during the window, including Tokio runtime primitives and SQLite's Rust
  wrapper objects.
- Excluded: allocations by native code that does not route through the Rust
  global allocator (notably SQLite's C-side memory), kernel page cache, and
  file-backed database storage; bytes freed for allocations made before the
  window opened are counted as drops inside the window, so `live_bytes` is a
  running balance of window allocations and window drops, not a strict heap
  snapshot, saturating at zero if pre-window drops would drive it negative;
  bytes freed after the window closes are not observed.
- Because the plan type is crate-private, an isolated plan-object footprint is
  not claimed; `peak_live_bytes` is the end-to-end complete-rebuild
  high-water, and `final_live_bytes` is the balance remaining at window close
  (including retained database-connection buffers). No byte value is
  converted into a timing target.
- The memory window is collected in a separate focused memory-only run
  (`cargo bench` with a no-match filter so no timed benchmark executes), whose
  stdout with true revision/date/command provenance tokens is recorded at
  `target/nav-memory-collection-output.txt`; the generator requires the values
  to be present and non-vacuous and, in results-only mode, requires the
  provenance tokens and never attributes that older measurement to a newer
  revision or reference time.

The performance generator rejects missing work evidence and rejects unequal
preparation-read counts. It does not invent timings, convert incomplete benchmark
collection into a pass, or alter immutable fixed targets. The generator rejects
zero-valued work or memory proxies so an inert metric callback cannot become a
vacuous pass. Any fixed-target regression requires resolution or an explicit
decision gate with provenance.

## Decision gate record (BLOCKED)

Four complete offline runs of `./generate-performance-baseline.sh`
(`CARGO_NET_OFFLINE=true`, samples 10, warm-up 0.1 s, measurement-time 0.1 s,
release build) were collected on a shared 12-core Orca execution host: runs 1–2
on 2026-09-22 and runs 3–4 on 2026-10-01. Runs 1–3 recorded **fixed-target
regressions** on `transaction_listing_representative` /
`transaction_listing_stress` paths, and run 1 also recorded one on
`rolling_metric_representative` (passed in run 2). Run 4 recorded regressions
on the same two `transaction_listing_representative` /
`transaction_listing_stress` paths. Per coordinator instruction (2026-10-01 checkpoint) no further benchmark
collection was performed after run 4; no failure was discarded, re-run until
green, or waived.

| Path | Run 1 p95 (ns) | Run 2 p95 (ns) | Run 3 p95 (ns) | Run 4 p95 (ns) | Immutable target (ns) |
| --- | --- | --- | --- | --- | --- |
| `transaction_listing` | 546,424.75 (failed) | 468,339.5 (passed) | passed | passed | 486,397 |
| `transaction_listing_representative` | 29,372,720 | 31,015,634 | 22,949,776 | 22,607,817 | 21,554,013 |
| `transaction_listing_stress` | 135,363,533 | 111,915,912 | 114,137,330 | 100,651,388 | 98,552,625 |
| `rolling_metric_representative` | 290,169.25 (failed) | 227,675.4 (passed) | passed | passed | 260,907 |

In all four runs every NAV-specific target passed:
`nav_rebuild_full`, `nav_rebuild_full_representative`, `nav_rebuild_full_stress`,
`nav_rebuild_incremental`, and `nav_readiness_warm_representative`, as well as
`startup_and_migration`. Small `transaction_listing` and
`rolling_metric_representative` failed in run 1 and passed in runs 2–4. Work
evidence was captured in every run: warm Historical preparation made zero source
calls, preparation `SELECT` counts were equal across one and twenty calendar
years (5 vs 5), and the allocation-work proxy recorded complete
representative and stress plan allocation-call counts (run 3: 2,467,579 /
9,655,915; run 4: 2,467,214 / 9,656,752 — small fixed-input drift between
runs from async/DB-runtime allocations inside the measured call-count window).
After the coordinator checkpoint stopped timing collection, byte-aware memory
profiles of the complete stress rebuild were collected in dedicated focused
memory-only windows. Three focused collections were taken; peak and final
live bytes reproduced exactly in all three (46,391,190 / 13,420,716), with
cumulative allocated/deallocated bytes drifting ~0.01% across runs. The
canonical evidence is the third collection, collected from the exact
commit-`3ff305e` bench content at `2026-10-01T18:05:42+02:00` and recorded with
its own `memory_collection_provenance` tokens (collected_at, revision,
command) in `target/nav-memory-collection-output.txt`; the generator requires
these tokens when it selects the focused collection in results-only mode and
does not infer the current revision or time for it. The first collection's
cumulative counts (1,377,071,225 / 1,363,650,509; 9,656,501 scoped calls)
remain recorded here. Mode selection in the generator: a full run must parse
the memory line from the current benchmark stdout of that same run; the
focused collection file may not override it, so stale focused evidence cannot
shadow fresh full-run output. The committed
`docs/performance-baseline-results.json` is the run-4 report: after the
2026-10-01 checkpoint stopped benchmark collection, a generator fix
(rejecting vacuous work proxies) was re-verified with
`PERFORMANCE_RESULTS_ONLY=1 ./generate-performance-baseline.sh`, which
re-derived the identical numbers in results-only mode from the run-4 Criterion
artifacts and run-4 benchmark stdout without any additional benchmark
collection; its `generation_mode` field records this results-only
re-derivation. One additional
partial, filtered diagnostic benchmark collection (subset
`transaction_listing`/`rolling_metric`) was taken on 2026-10-01 to probe
variance; it is not a complete run and is recorded here only as an incomplete
collection without substituting any missing evidence.

Provenance and scope notes:

- The #68 change modifies only `benches/performance.rs`, the performance-harness
  test, `generate-performance-baseline.sh`, and documentation; it touches no
  transaction-listing, rolling-metric, or NAV source behavior. The failing
  targets exercise code paths unrelated to the prepared-NAV rollout.
- The failures vary between back-to-back runs on the same machine, consistent
  with shared-host load noise on micro-benchmarks rather than a code regression.
  This is an observation, not a proven root cause and not a basis for a pass.
- The listed targets are immutable user-approved issue #20 values; no target
  change was made and no rerun was discarded or cherry-picked. After the
  2026-10-01 coordinator checkpoint, benchmark collection was stopped, remains
  stopped, and no waiver or merge was authorized for the residual failures.
- Because acceptance requires each fixed-target regression to be resolved or
  explicitly decided before rollout completion, the direct rollout is **BLOCKED
  at the decision gate**. Accepting these failures or changing the targets
  requires explicit user approval after PR review. The 2026-10-01 coordinator
  checkpoint instructed: targets stay immutable, no waiver, no merge, benchmark
  collection stopped, and issue #68 / the direct rollout must not be claimed
  complete. Nothing in this document or in
  `docs/performance-baseline-results.json` claims the performance report
  verification or the rollout gate passed; the generated report records
  `decision_gate_status: "failed"` with the exact failing paths.

## Recorded user approval (2026-10-01)

The user explicitly agreed, in the coordinator conversation on 2026-10-01, to
accept the latest `transaction_listing_representative` (~4.9%) and
`transaction_listing_stress` (~2.1%) misses from run 4 as a **documented
rollout exception**, conditional on PR review and checks. Scope of this
record:

- The immutable issue #20 targets and every row of the four-run failure
  record above remain unchanged; no target was edited, re-derived, or waived
  in the harness or generated report.
- The generated measurements and
  `docs/performance-baseline-results.json` continue to show the actual failed
  comparisons (`decision_gate_status: "failed"`); the user acceptance is
  separate, recorded human approval found in the run provenance, not a
  generated pass result.
- No proven noise root cause is asserted: the shared-host-load explanation
  remains an observation. Run-to-run variance and both historical
  (`transaction_listing`, `rolling_metric_representative`) failures stay
  disclosed.
- Benchmark collection is stopped; no further collection or merge has been
  authorized by this approval.
- Follow-up investigation of transaction-listing measurement variability is
  tracked separately in `.scratch/transaction-listing-variability/PRD.md`
  (investigation only, no implementation expansion).

## Verification

```text
CARGO_NET_OFFLINE=true cargo fmt --check
CARGO_NET_OFFLINE=true cargo clippy --all-targets -- -D warnings
CARGO_NET_OFFLINE=true cargo test --workspace
```

The generator itself runs `cargo clippy --offline -- -D warnings` and
`cargo test --offline` and records those exact commands in
`docs/performance-baseline-results.json` (`verification.commands`); the
stronger `--all-targets` / `--workspace` variants above were additionally run
on this branch with identical passing results.

The committed report must identify whether full Criterion collection completed;
historical R77 evidence in `docs/replay-performance-evidence.md` is not substituted
for #68 measurements.
