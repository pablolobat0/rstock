# 02 — Trust the NAV rebuild checkpoint

## Parent

#62

## What to build

Make routine NAV readiness treat the latest Complete NAV snapshot as the authoritative NAV rebuild checkpoint. Warm readiness and incremental extension must use that checkpoint directly instead of replaying and comparing all earlier Transaction ledger holdings and per-asset snapshots, so routine work is proportional to the checkpoint and missing range rather than complete historical detail.

## Acceptance criteria

- [ ] Routine readiness no longer scans all historical snapshot dates, all earlier per-asset quantities, or the complete ledger to audit persisted history.
- [ ] A current history returns its latest snapshot without work that grows with the number of earlier Complete NAV snapshots or historical holding-days.
- [ ] An incremental rebuild initializes NAV, outstanding shares, accumulated dividend cash, and holdings from the latest checkpoint and resumes on the following calendar date.
- [ ] A fresh portfolio without a checkpoint still initializes from its first relevant Transaction ledger entry and preserves the initial NAV seed behavior.
- [ ] Tests compare otherwise equivalent short and long histories and prove warm-readiness query work does not scale with earlier history.
- [ ] Tests that expected routine readiness to repair manually altered or legacy earlier snapshots are removed or rewritten to match checkpoint trust.
- [ ] Recording, editing, deleting, or importing Transaction ledger entries continues to invalidate every dependent Complete NAV snapshot atomically.
- [ ] No explicit full-history verification command, compatibility layer, generation marker, or schema migration is introduced.
- [ ] Existing NAV consumers continue to use the same public readiness interface and receive the same readiness result shape.
- [ ] Formatting, linting with warnings denied, and the complete test suite pass.

## Blocked by

- #63
