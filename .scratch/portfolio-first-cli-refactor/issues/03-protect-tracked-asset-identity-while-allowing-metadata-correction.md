# Protect Tracked Asset Identity While Allowing Metadata Correction

Status: done

## Parent

.scratch/portfolio-first-cli-refactor/PRD.md

## What to build

Keep the user-facing identity, vehicle type, and currency of a Tracked asset stable after creation while still allowing corrections to descriptive metadata, Asset classification, and provider lookup metadata. When a fund/ETF Morningstar code changes, invalidate cached price data that was tied to the old provider lookup identity.

## Acceptance criteria

- [ ] Tracked asset ticker/ISIN remains immutable after creation.
- [ ] Tracked asset vehicle type remains immutable after creation.
- [ ] Tracked asset currency remains immutable after creation.
- [ ] Tracked asset name remains editable.
- [ ] Tracked asset Asset classification remains editable and is validated with the same class-specific consistency rules used at creation/import time.
- [ ] Fund/ETF Morningstar code remains editable.
- [ ] Editing a fund/ETF Morningstar code invalidates cached price data associated with the asset.
- [ ] Editing non-provider descriptive metadata does not unnecessarily invalidate price cache data.
- [ ] Tests cover allowed edits, rejected identity mutations, and Morningstar-code cache invalidation.

## Blocked by

None - can start immediately
