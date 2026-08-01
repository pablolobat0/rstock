# 02 — Give the NAV module readiness ownership

**What to build:** Make NAV/history readiness an outcome owned by the NAV module so every NAV consumer receives current reproducible history without first running `get` or knowing a portfolio-specific rebuild sequence.

**Blocked by:** 01 — Establish a deterministic current-date seam

**Status:** ready-for-agent

- [ ] The NAV module exposes one readiness operation that uses the injected clock and preserves existing incremental rebuild behavior.
- [ ] The full Portfolio view requests NAV readiness before reading NAV, return, and risk facts.
- [ ] Portfolio-NAV analysis requests NAV readiness without requiring the Portfolio view to be built first.
- [ ] Analysis that relies on generated portfolio history obtains readiness through the NAV module rather than through a portfolio-specific prerequisite.
- [ ] Current-position and asset-series operations do not request NAV readiness when they do not consume NAV/history.
- [ ] NAV unitization, Effective valuation date, Historical market data, and Forward-filled market data semantics remain unchanged.
- [ ] Tests prove a NAV consumer works correctly when invoked before `get` and uses the fixed clock cutoff.
- [ ] Tests prove a non-NAV consumer does not rebuild NAV history.
- [ ] Formatting, strict linting, and the complete test suite pass.
