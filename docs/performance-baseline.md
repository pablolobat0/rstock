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

The Criterion groups currently measure transaction listing and cold/warm
historical market-data preparation. Source calls, requested intervals, active
requests, and peak concurrency are recorded by the offline source counters.
The remaining audited paths (portfolio retrieval, full/incremental NAV rebuild,
rolling and matrix correlation, and startup) are named decision-gate paths but
have no accepted timing result until their dedicated benchmark cases are run on
the review machine. Earlier audit observations that provider latency, large
history rebuild cost, memory pressure, or migration I/O are material are
hypotheses, not measurements from this artifact.

## Decision gate (pending approval)

No dependent optimization may start until repeated Criterion samples are
reviewed and the coordinator approves path-specific targets. Proposed starting
targets are: reduce audited source/SQLite work counts for every optimization;
hold representative medians within 10% until evidence supports a stricter
target; and investigate only paths whose p50 or p95 contribution is material to
the end-to-end command. The proposed fixed internal source concurrency limit is
**4** (not user-configurable or source-specific); this is a proposal, not an
approved decision, and must be confirmed at the gate from peak activity and
latency samples.

## Verification

Run offline with `cargo bench --bench performance`, then `cargo fmt`,
`cargo clippy -- -D warnings`, and `cargo test`. No user database or network is
used by the harness.
