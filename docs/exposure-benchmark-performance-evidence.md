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

The procedure uses an in-memory database with the production migrations, dummy
`XPERF###` identities, a fixed clock, and injected offline source
observations. It never opens the user database and never contacts a public
market-data endpoint.

Two properties make the measurement trustworthy:

- **The measured window excludes the fixture construction.** Every fixture is
  fully built, including its persisted Complete NAV history, before the
  allocator window opens.
- **The measurement is isolated.** The performance file holds exactly one
  test, so no other test of the process allocates inside the measured window.
  The counting allocator observes every allocation of the process, so a
  parallel test would otherwise pollute the peak live bytes.

## Fixtures

| Fixture | Holdings | Closing snapshots | Persisted exposure rows | Contributing references |
| --- | --- | --- | --- | --- |
| Short | 4 | 41 | 164 | 1 |
| Long | 4 | 401 | 1,604 | 1 |
| Representative | 10 | 366 | 3,660 | 1 |
| Stress | 50 | 3,651 | 182,550 | 1 |

Both the short and the long fixture hold four assets, so their difference
isolates the effect of the number of closing dates. The representative and
stress fixtures scale the holdings and the history together.

## Recorded measurements

Release build, one run, printed by the performance test:

| Measurement | Short | Long | Representative | Stress |
| --- | ---: | ---: | ---: | ---: |
| Elapsed wall time of the comparison (ms) | 1.72 | 8.86 | 17.53 | 740.56 |
| Elapsed wall time of one separate bounded exposure read (ms) | 0.59 | 4.31 | 9.95 | 501.12 |
| Persisted exposure rows the bounded read returns | 164 | 1,604 | 3,660 | 182,550 |
| SQL read statements issued by the comparison | 7 | 7 | 7 | 7 |
| Historical source attempts (history requests and predecessor lookups) | 2 | 2 | 2 | 2 |
| Allocation calls in the measured window | 5,551 | 37,327 | 57,360 | 2,039,202 |
| Allocated bytes in the measured window | 432,530 | 2,582,999 | 3,694,245 | 130,940,587 |
| Deallocated bytes in the measured window | 408,681 | 2,523,273 | 3,634,918 | 130,578,901 |
| Peak live bytes in the measured window | 116,699 | 517,910 | 1,159,606 | 59,494,326 |
| Final live bytes at window close | 23,849 | 59,726 | 59,327 | 361,686 |

The timings are **elapsed wall time** of the measured call, not CPU time. The
bounded exposure read is a **separate measurement of that read**; it is not
subtracted from the comparison timing, because the two calls do not share one
instrumented scope and a subtraction would claim a split the measurement does
not establish.

### Memory scope and exclusions

- Measured process memory: bytes routed through the Rust `GlobalAlloc`
  implementation on the test binary's counting allocator. It is an
  allocation-level high-water and live profile, not an RSS or page-level
  end-to-end memory measurement.
- Measured window: from opening the window (after the fixture is fully
  constructed) until the comparison returns. The comparison's own
  allocations, the Tokio runtime's allocations inside the window, and
  SQLite's Rust-side row objects are included.
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
  number of SQL read statements (7) for a 41-day window and a 401-day window.
  The per-asset exposures are read with one ordered range query over the
  comparison window, and the reference prices are read with one ordered range
  query per reference.
- **Bounded source attempts, not one per date.** The comparison makes exactly
  two source attempts per contributing reference for the whole window: one
  history request and one predecessor lookup. Both are counted; the
  predecessor lookup is a source attempt too and is included in the recorded
  numbers. A reference that stops being held is prepared only for the window
  it was held in, so a closed position never pulls a full-window request.
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
convention — costs about 741 ms of elapsed wall time and about 59.5 MB of peak
live bytes for one dashboard comparison, dominated by materializing 182,550
persisted exposure rows. The representative fixture costs about 18 ms.

That cost is reported as evidence rather than resolved by changing the
calculation:

- Switching to monthly exposure weights would silently change the measured
  result: the PRD requires daily prior-closing exposures, and a monthly
  weight would hide the same-day-trade effect and the closed-position
  contribution that the acceptance criteria require.
- No millisecond or memory budget was ever agreed with the user, so this
  document does not declare the stress cost a failure.
- The cost is escalated to the coordinator as a decision input. The available
  reduction, if the decision is to reduce cost, is an implementation detail (a
  narrower exposure read that does not materialize every row) rather than a
  change in the comparison's semantics.

## Reproducing the structural evidence

The same file also asserts the structural properties above as an ordinary
test, so a regression fails the offline suite:

- `representative_and_stress_work_evidence_is_recorded` — the recorded
  representative and stress work evidence with no invented threshold, and the
  equal read-statement and source-attempt counts for the 41-day and 401-day
  windows.

The NAV-readiness separation is asserted by
`the_comparison_performs_no_second_nav_rebuild` in
`tests/exposure_benchmark_tests.rs`.
