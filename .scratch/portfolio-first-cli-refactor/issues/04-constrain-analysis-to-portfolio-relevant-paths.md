# Constrain Analysis To Portfolio-Relevant Paths

Status: done

## Parent

.scratch/portfolio-first-cli-refactor/PRD.md

## What to build

Constrain read-only analysis commands to portfolio-relevant analysis. Keep top-level `analyze`, keep standalone fund analysis as Fund candidate analysis, make rolling correlation compare Tracked assets rather than arbitrary tickers, and remove the monitor runtime feature while preserving historical migrations.

## Acceptance criteria

- [ ] The `analyze` command group remains available.
- [ ] `analyze fund --code` remains available as Fund candidate analysis.
- [ ] Rolling correlation resolves both inputs as Tracked assets using user-facing ticker/ISIN identity.
- [ ] Rolling correlation uses ticker lookup for stock Tracked assets.
- [ ] Rolling correlation uses stored Morningstar code lookup for fund/ETF Tracked assets.
- [ ] Rolling correlation rejects unknown identifiers that are not Tracked assets.
- [ ] Monitor commands are removed from the CLI.
- [ ] Monitor runtime modules, display code, service/repo exposure, constants, and tests that become dead are removed or updated.
- [ ] Historical migrations, including watchlist-related migrations, are not rewritten.
- [ ] No market-data refresh, clear, or cache-management CLI is added.
- [ ] Tests cover rolling correlation for Tracked assets and removed/updated monitor behavior without network calls.

## Blocked by

None - can start immediately
