# 06 — Route composition through current positions

**What to build:** Make portfolio composition describe current Transaction ledger inventory through the focused current-positions outcome, without rebuilding NAV or silently calculating allocations from only the holdings that can be valued.

**Blocked by:** 05 — Make unavailable positions and aggregates explicit

**Status:** ready-for-agent

- [ ] Composition consumes the focused current-positions outcome rather than the broad full Portfolio view result.
- [ ] Composition includes a holding bought after the latest Effective valuation date.
- [ ] Requesting composition does not create or rebuild NAV history.
- [ ] Complete current valuation produces the same Portfolio-relevant classification and look-through analysis expected today.
- [ ] If any included performance holding has no Individual price, value-dependent composition facts are unavailable rather than calculated from a subset.
- [ ] The unavailable composition outcome explains the relevant current-position Market data limitation without implying NAV is invalid.
- [ ] Monetary holdings remain excluded from composition weights.
- [ ] Human and JSON composition output represent unavailable value-dependent analysis consistently.
- [ ] Tests cover current post-snapshot inventory, complete composition, one unvalued holding, and absence of NAV rebuild side effects.
- [ ] Formatting, strict linting, and the complete test suite pass.
