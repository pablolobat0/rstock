# 04 — Correct current-holding financial facts

**What to build:** Make every current holding use the same professional financial semantics: weighted Average cost for remaining inventory, lifetime dividends as a separate fact, and Open-position gain/loss that excludes dividends and sold units.

**Blocked by:** 03 — Add a focused current-positions outcome

**Status:** ready-for-agent

- [ ] Average cost is weighted by acquired quantity and includes buy fees in Base currency.
- [ ] A partial sell removes remaining cost proportionally without assigning explicit tax lots.
- [ ] A split changes quantity and Average cost per unit without changing total remaining cost.
- [ ] Dividend income is recorded as lifetime net income after fees and remains separate from Open-position gain/loss.
- [ ] Open-position gain/loss equals current Base currency value minus remaining Average cost.
- [ ] Open-position gain/loss excludes realized gain or loss from sold units.
- [ ] Performance positions and Monetary holdings apply identical financial rules.
- [ ] Human output labels Open-position gain/loss unambiguously and displays dividends separately.
- [ ] JSON position facts preserve separate dividend and Open-position gain/loss values.
- [ ] Interface-level scenarios cover multiple buys, fees, a partial sell, a split, and a dividend before a partial sell.
- [ ] Existing tests that encode dividends inside gain/loss are corrected to the accepted domain semantics.
- [ ] Formatting, strict linting, and the complete test suite pass.
