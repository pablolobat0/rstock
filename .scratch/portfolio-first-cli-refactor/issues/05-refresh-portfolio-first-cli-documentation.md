# Refresh Portfolio-First CLI Documentation

Status: needs-triage

## Parent

.scratch/portfolio-first-cli-refactor/PRD.md

## What to build

Refresh project documentation so future maintainers and agents see the portfolio-first command surface and domain vocabulary that now matches the implementation. Update stale command descriptions and any affected architecture/conventions notes after the behavior changes land.

## Acceptance criteria

- [ ] Documentation describes `get` and `portfolio get` as the dashboard paths.
- [ ] Documentation describes Transaction ledger commands under `transaction`, including list/import/export.
- [ ] Documentation no longer describes removed top-level `buy`, `portfolio list`, broad `data`, or monitor commands as active CLI features.
- [ ] Documentation describes rolling correlation as comparing Tracked assets.
- [ ] Documentation describes transaction CSV import/export as using the new classified Tracked asset schema.
- [ ] Documentation uses the resolved domain vocabulary consistently: Portfolio-relevant analysis, Fund candidate, Asset classification, Tracked asset, and Transaction ledger.
- [ ] Stale module descriptions discovered during exploration are corrected where touched by this CLI refactor.
- [ ] Verification commands pass after documentation updates.

## Blocked by

- .scratch/portfolio-first-cli-refactor/issues/01-reshape-transaction-ledger-cli.md
- .scratch/portfolio-first-cli-refactor/issues/02-require-classified-tracked-assets-across-manual-and-csv-workflows.md
- .scratch/portfolio-first-cli-refactor/issues/03-protect-tracked-asset-identity-while-allowing-metadata-correction.md
- .scratch/portfolio-first-cli-refactor/issues/04-constrain-analysis-to-portfolio-relevant-paths.md
