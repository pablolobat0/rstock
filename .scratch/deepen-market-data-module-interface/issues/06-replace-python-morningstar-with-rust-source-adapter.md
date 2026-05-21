# Replace Python Morningstar with Rust source Adapter

Status: ready-for-agent

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Replace the Python Morningstar scripts with Rust implementation inside the market data Module. Preserve the current Morningstar chartservice and sal-service endpoints while moving token handling, JSON parsing, fund price history, and fund data into the private Morningstar source **Adapter**. Holdings-only callers should call `MarketData::fund_data(code, limit)` and use the returned `holdings` field rather than using a separate holdings method.

## Acceptance criteria

- [ ] Morningstar fund and ETF **Historical market data** uses Rust HTTP implementation instead of `uv run scripts/get_fund_price_history.py`.
- [ ] The Rust Morningstar HTTP implementation uses `reqwest` with rustls.
- [ ] Fund holdings used by composition come from `MarketData::fund_data(code, limit).holdings` instead of `uv run scripts/get_fund_holdings.py`.
- [ ] **Fund candidate** analysis uses Rust HTTP implementation instead of `uv run scripts/get_fund_data.py`.
- [ ] Morningstar chartservice and sal-service endpoints, query parameters, and parsed fields preserve current behaviour.
- [ ] Morningstar token scraping, JWT expiry parsing, persistent cache at `~/.rstock/cache/morningstar_token.json`, and `401` refresh are implemented as private Adapter details.
- [ ] Token cache writes are best-effort and warn through `tracing::warn!` without failing the user operation.
- [ ] Rust parsing tests use static payloads and do not make network calls.
- [ ] All Python-backed Morningstar call paths are replaced by Rust implementation.

## Blocked by

- .scratch/deepen-market-data-module-interface/issues/00-introduce-stateful-market-data-and-sources.md
