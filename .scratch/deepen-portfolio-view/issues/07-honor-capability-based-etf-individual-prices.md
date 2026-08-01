# 07 — Honor capability-based ETF Individual prices

**What to build:** Let an ETF use a Live quote for its Individual price when the existing Market data source path can supply one, while preserving Historical market data fallback when no Live quote is supported.

**Blocked by:** 01 — Establish a deterministic current-date seam

**Status:** ready-for-agent

- [ ] An ETF Individual price uses a same-day Live quote when the injected Market data source supplies one through the existing source coordination seam.
- [ ] An ETF without a supported Live quote falls back to its latest available Historical market data.
- [ ] Mutual funds continue to use closing-price Historical market data semantics.
- [ ] Stocks continue to use current Live quote behavior when available.
- [ ] ETF fallback retains its Historical market data price date and applicable Market data limitation values.
- [ ] The implementation does not expose private Yahoo Finance or Morningstar adapters outside the market data module.
- [ ] The implementation does not add a new Market data source, ETF symbol mapping, or provider-specific lookup metadata solely for Live quotes.
- [ ] Fixed-clock tests cover an ETF with a supplied Live quote, an ETF requiring fallback, a mutual fund, and a stock.
- [ ] ADR-0001 source coordination remains intact.
- [ ] Formatting, strict linting, and the complete test suite pass.
