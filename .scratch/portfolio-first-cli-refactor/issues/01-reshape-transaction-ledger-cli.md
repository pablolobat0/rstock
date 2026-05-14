# Reshape Transaction Ledger CLI

Status: needs-triage

## Parent

.scratch/portfolio-first-cli-refactor/PRD.md

## What to build

Reshape the command surface so Transaction ledger maintenance lives under the singular `transaction` group while the portfolio dashboard remains available through both `get` and `portfolio get`. Remove command paths that blur the portfolio-first boundary, add transaction listing so edit/delete IDs are discoverable, and enforce consistent transaction numeric validation through user-facing transaction entry paths.

## Acceptance criteria

- [ ] `get` and `portfolio get` both continue to show the portfolio dashboard with NAV chart.
- [ ] Top-level `buy` is removed, while `transaction buy` remains available.
- [ ] `portfolio list` is removed and no replacement asset-list command is added.
- [ ] Transaction CSV import/export commands are available under `transaction import` and `transaction export` instead of the broad `data` group.
- [ ] `transaction list` displays transaction IDs and enough transaction detail to support edit/delete workflows.
- [ ] `transaction edit` remains limited to date, quantity, price, and fees.
- [ ] `transaction delete` remains available with confirmation behavior intact.
- [ ] Transaction quantities, prices, dividend amounts, and split ratios are rejected when non-positive.
- [ ] Transaction fees are rejected when negative.
- [ ] Dividend amount continues to mean total cash received.
- [ ] Split ratio continues to mean new units per old unit.
- [ ] Tests cover the accepted and rejected command paths and transaction-listing behavior without making network calls.

## Blocked by

None - can start immediately
