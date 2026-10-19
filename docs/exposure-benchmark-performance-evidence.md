# Exposure-Matched Benchmark Performance Evidence

This document is the evidence record for issue #117. The exposure-matched
portfolio comparison is the primary benchmark of the Portfolio dashboard, and
its cost is recorded **separately from the NAV rebuild**: the measurements
below run against an already ready Complete NAV history, so no ledger replay,
no NAV rebuild, and no snapshot write is part of any measured window.

The numbers are work evidence, not a performance budget. The user never set a
millisecond, memory, read-count, or source-attempt target, so this document
records measurements and structural properties; it does not invent a pass or
fail threshold, and it does not substitute monthly exposure weights for daily
ones. The measured cost of a deliberately pessimistic stress fixture is
reported as a material cost concern below.

## Procedure

Run from the integration branch head with networking disabled:

```text
CARGO_NET_OFFLINE=true cargo test --offline --release \
  --test exposure_benchmark_performance_tests -- --nocapture
```

The procedure uses an in-memory SQLite database with the production
migrations, dummy `XPERF###` identities, a fixed clock, and injected offline
source observations. It never opens the user database and never contacts a
public market-data endpoint. The fixtures are constructed **outside** every
measured window.

## Fixtures

| Fixture | Holdings | Closing snapshots | Persisted exposure rows | Contributing references |
| --- | --- | --- | --- | --- |
| Representative | 10 | 366 (one year) | 3,660 | 1 |
| Stress | 50 | 3,651 (ten years) | 182,550 | 1 |

Both fixtures persist a ready Complete NAV history — the `portfolio_history`
and `portfolio_asset_history` rows the NAV rebuild would have written — and one
verified reference assigned to every holding. The comparison therefore reads
an already ready history exactly as the Portfolio dashboard does after NAV
readiness.

## Recorded measurements

Release build, one run, same host, printed by the performance test:

| Measurement | Representative | Stress |
| --- | --- | --- |
| Comparison CPU (whole call) | 16.3 ms | 722.1 ms |
| Bounded exposure read CPU (of the above) | 11.2 ms | 504.8 ms |
| Comparison CPU without the bounded read | 5.1 ms | 217.3 ms |
| SQL read statements issued by the comparison | 7 | 7 |
| Historical source attempts | 1 | 1 |
| Allocation calls in the measured window | 62,232 | 2,411,553 |
| Allocated bytes in the measured window | 3,705,025 | 135,579,521 |
| Deallocated bytes in the measured window | 3,626,796 | 135,100,679 |
| Peak live bytes in the measured window | 1,159,126 | 59,494,326 |
| Final live bytes at window close | 78,229 | 478,842 |

The measurement split is material: the bounded read of the persisted exposure
history is about 70% of the comparison's cost, and the remaining calculation
(reference preparation, weight aggregation, wealth compounding, and the
standard rows) is about 30%.

### Memory scope and exclusions

- Measured process memory: bytes routed through the Rust `GlobalAlloc`
  implementation on the test binary's counting allocator. It is an
  allocation-level high-water and live profile, not an RSS or page-level
  end-to-end memory measurement.
- Measured window: from opening the window (after the fixture is fully
  constructed) until the comparison returns. Fixture construction, the
  reference preparation performed inside the window, and the Tokio runtime's
  own allocations inside the window are included.
- Excluded: allocations by native code that does not route through the Rust
  global allocator (notably SQLite's C-side memory), the kernel page cache,
  and the fixture's own persisted data. Bytes freed for allocations made
  before the window opened are counted as drops inside the window, so
  `final_live_bytes` is a running balance of window allocations and window
  drops, not a strict heap snapshot, and it saturates at zero when pre-window
  drops would drive it negative.
- The peak live bytes are dominated by the materialized per-asset exposure
  rows of the comparison window (182,550 rows in the stress fixture). No
  byte value is converted into a timing target.

## Structural properties (asserted, not merely measured)

- **Bounded reads, not one query per date.** The comparison issues the same
  number of SQL read statements (7) for a 40-day window and a 400-day window.
  The per-asset exposures are read with one ordered range query over the
  comparison window, and the reference prices are read with one ordered range
  query per reference.
- **One source attempt per reference, not one per date.** The comparison makes
  exactly one Historical source request per contributing reference for the
  whole window. A reference that stops being held is prepared only for the
  window it was held in, so a closed position never pulls a full-window
  request.
- **No second NAV rebuild.** The comparison writes no NAV history: the
  persisted snapshot count is unchanged after the comparison, and NAV
  readiness on the untouched history reports zero execution reads. The
  incremental cost above is therefore additional to the NAV rebuild, whose own
  cost is recorded separately in
  [nav-rollout-performance-evidence.md](nav-rollout-performance-evidence.md).
- **No ledger replay.** The comparison reads the persisted ledger projection
  (the Complete NAV snapshots and their per-asset exposures); it never
  re-derives quantities, remaining cost, or transaction validity.

## Material cost concern (escalated, not silently changed)

The stress fixture — 50 holdings held continuously for ten years, which is
larger than a personal portfolio but within the repository's stress-fixture
convention — costs about 722 ms of CPU and about 59 MB of peak live bytes for
one dashboard comparison, dominated by reading 182,550 persisted exposure
rows. The representative fixture costs about 16 ms.

That cost is reported as evidence rather than resolved by changing the
calculation:

- Switching to monthly exposure weights would silently change the measured
  result: the PRD requires daily prior-closing exposures, and a monthly
  weight would hide the same-day-trade effect and the closed-position
  contribution that the acceptance criteria require.
- No millisecond or memory budget was ever agreed with the user, so this
  document does not declare the stress cost a failure.
- The cost is escalated to the coordinator as a decision input. The available
  reductions, if the decision is to reduce cost, are implementation details
  (a narrower exposure read shape) rather than a change in the comparison's
  semantics.

## Reproducing the structural evidence

The same file also asserts the structural properties above as ordinary tests,
so a regression fails the offline suite:

- `exposure_comparison_work_does_not_scale_with_the_number_of_dates` — equal
  read-statement and source-attempt counts for a 40-day and a 400-day window.
- `exposure_comparison_performs_no_second_nav_rebuild` — the persisted history
  is untouched and NAV readiness reports zero execution reads afterwards.
- `representative_and_stress_measurements_are_recorded` — the recorded
  representative and stress work evidence, with no invented threshold.
