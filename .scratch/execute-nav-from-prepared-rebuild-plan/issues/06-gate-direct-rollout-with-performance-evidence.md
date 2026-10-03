# 06 — Gate direct rollout with performance evidence

## Parent

#62

## What to build

Complete the direct rollout of the prepared-plan NAV engine by removing obsolete rebuild and full-audit paths, measuring the final behavior with the repository's established offline performance harness, and aligning architecture, conventions, and performance documentation. The prepared-plan implementation must become the sole production path without a feature flag or compatibility period.

## Acceptance criteria

- [ ] The prepared-plan engine is the only NAV rebuild path; obsolete preparation, execution, fallback, and full-history audit code is removed.
- [ ] No feature flag, shadow calculation, dual engine, compatibility wrapper, explicit verification command, or concurrency-generation guard is introduced.
- [ ] Existing full, incremental, and warm NAV readiness Criterion paths run against the prepared-plan implementation.
- [ ] Representative and 100-asset, 20-year stress rebuild paths provide actual timing evidence using the established fixture construction and fixed clock.
- [ ] A deterministic allocation or peak-memory proxy records the complete plan's memory behavior on the stress fixture.
- [ ] Captured work evidence confirms zero database reads during plan execution and preparation query counts that do not grow with rebuilt calendar dates or holding-days.
- [ ] Fully warm Historical market data preparation continues to make zero source calls.
- [ ] Existing immutable performance targets are preserved and evaluated by the established report generator; no timing target or pass result is fabricated.
- [ ] Any fixed-target regression is resolved or taken through an explicit decision gate with recorded provenance before rollout is considered complete.
- [ ] Benchmark fixture construction remains outside timed closures and uses only dummy identities, fake or unreachable sources, and temporary databases.
- [ ] The performance procedure does not access the user database or make network calls.
- [ ] Architecture and convention documentation describes the NAV rebuild checkpoint, prepared plan ownership, Positive-holding intervals, contiguous calculable prefix, and atomic date-and-row-bounded persistence.
- [ ] Performance documentation records the final measurement method, results provenance, and any incomplete benchmark collection without inferring missing evidence.
- [ ] Public NAV readiness retains exact snapshot equivalence for existing calculable cases and the complete test suite covers the accepted new behavior.
- [ ] Formatting, linting with warnings denied, the complete offline test suite, and the repository's performance report verification pass.

## Blocked by

- #66
- #65
- #67
- #64
