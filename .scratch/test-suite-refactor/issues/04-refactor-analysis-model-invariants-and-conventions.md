Status: needs-triage

# Refactor Analysis, Model Invariants, and Conventions

## Parent

.scratch/test-suite-refactor/PRD.md

## What to build

Finish the test-suite refactor by moving **Portfolio-relevant analysis** and pure model/math invariant tests into their final domain buckets, removing stale files and helpers, updating the existing testing conventions, and running final verification. The completed slice should leave the suite in the target six-bucket shape with conventions that match the new scenario-builder approach.

## Acceptance criteria

- [ ] Composition, correlation, fund analysis, metrics, and monitor tests are moved or reshaped into the analysis behavior bucket.
- [ ] Pure conversion and math invariant tests are moved or reshaped into the model invariant bucket under the integration test suite.
- [ ] Simple pure or low-setup variants use table-driven tests where it improves clarity; complex database-backed scenarios remain separate named tests.
- [ ] Stale test files, unused helpers, and old layout remnants are removed after coverage is preserved in the new buckets.
- [ ] Existing testing conventions are updated to describe the domain test buckets, split common support, canonical fixture data, service-backed scenario builders, and assertion policy.
- [ ] Final verification passes with formatting, clippy warnings denied, and the full test suite.

## Blocked by

- .scratch/test-suite-refactor/issues/01-establish-domain-test-harness.md
- .scratch/test-suite-refactor/issues/02-refactor-valuation-and-market-data-tests.md
- .scratch/test-suite-refactor/issues/03-refactor-ledger-and-portfolio-view-tests.md
