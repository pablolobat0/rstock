Status: needs-triage

# Establish Domain Test Harness

## Parent

.scratch/test-suite-refactor/PRD.md

## What to build

Create the domain-oriented test harness that future test refactors can use end-to-end. The harness should provide split common test support, canonical fake **Tracked asset** data, deterministic date fixtures, a configurable mock price fetcher, domain-specific assertion helpers, and service-backed scenario builders. Migrate a small representative test path to prove the harness works without restructuring the whole suite in this slice.

## Acceptance criteria

- [ ] Common test support is split by responsibility for database setup/raw helpers, scenario builders, assertion helpers, and mock fetching.
- [ ] Canonical fake **Tracked asset** identities and fixed date fixtures exist for EUR stock, USD stock, EUR fund, EUR ETF, weekday, weekend, and forward-fill scenarios.
- [ ] Domain-specific assertion helpers exist for identifiers/exact fields, money values, NAV/ratio values, and general floating-point metrics.
- [ ] Scenario builders default to production services for normal **Transaction ledger** workflows and still expose raw database setup for persistence-focused or intentionally impossible states.
- [ ] A small representative existing test path uses the new harness and passes, proving the harness can support the later domain bucket migrations.
- [ ] No network calls are introduced; provider behavior remains controlled through the mock fetcher.

## Blocked by

None - can start immediately
