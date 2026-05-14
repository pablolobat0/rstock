# Make Market data limitation Actionable-Only

Status: needs-triage

Type: AFK

## Parent

`.scratch/deepen-historical-market-data/PRD.md`

## What to build

Make Market data limitation values represent only user-actionable market-data problems. Remove caller-visible source-versus-cache mechanics and non-actionable Acceptable Morningstar lag from the public limitation result, while preserving current NAV, Individual price, and warning behaviour where it remains actionable.

## Acceptance criteria

- [ ] Public Market data limitation results no longer expose whether a limitation came from source lag or cached fallback.
- [ ] Acceptable Morningstar lag does not create a returned Market data limitation.
- [ ] Excessive Morningstar lag still creates an actionable Market data limitation.
- [ ] Stock and FX stale-data limitations still use Completed weekday cadence.
- [ ] FX Market data limitation subjects use the non-Base currency rather than provider-specific pair strings.
- [ ] Existing user-facing warnings still appear for actionable limitations.
- [ ] Tests cover actionable-only limitations, Acceptable Morningstar lag suppression, excessive Morningstar lag, and FX subject wording.

## Blocked by

None - can start immediately
