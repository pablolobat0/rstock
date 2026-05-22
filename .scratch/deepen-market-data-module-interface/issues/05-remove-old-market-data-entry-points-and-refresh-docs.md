# Remove old market data entry points and refresh docs

Status: ready-for-agent

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Finish the market data architecture deepening by removing or privatizing obsolete low-level public helpers, deleting obsolete fetcher/subprocess entry points, ensuring callers import through the market data Module root, and keeping implementation documentation minimally correct after Python removal.

## Acceptance criteria

- [ ] Obsolete low-level public helpers are deleted or made private after all callers move to the new market data Interface.
- [ ] The old public `PriceFetcher` shape is removed after callers use `MarketData` and `MarketDataSources`.
- [ ] No Python scripts directory, `uv` subprocess references, or Python fallback paths remain after Rust feature parity exists.
- [ ] `utils::resolve_scripts_dir()` and script-resolution documentation are deleted if no non-Python code still uses them.
- [ ] `AGENTS.md` no longer describes fund/ETF price fetching as Python scripts via `uv run` after the Rust source **Adapter** is active.
- [ ] Callers import market-data behaviour through the market data Module root rather than internal submodules.
- [ ] Public market-data result types live in the shared model layer; private helper structs stay inside implementation submodules.
- [ ] `docs/ARCHITECTURE.md` no longer describes fund/ETF price fetching as Python scripts via `uv run` or `RSTOCK_SCRIPTS_DIR` after the Rust source **Adapter** is active.
- [ ] Tests exercise public market data Interfaces and do not depend on private helper paths.
- [ ] `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` pass.

## Blocked by

- .scratch/deepen-market-data-module-interface/issues/02-move-individual-price-behind-market-data-interface.md
- .scratch/deepen-market-data-module-interface/issues/03-add-cache-first-correlation-market-data-interface.md
- .scratch/deepen-market-data-module-interface/issues/04-route-remaining-analytics-through-correlation-market-data.md
- .scratch/deepen-market-data-module-interface/issues/06-replace-python-morningstar-with-rust-source-adapter.md
