# Remove old market data entry points and refresh docs

Status: done

## Parent

.scratch/deepen-market-data-module-interface/PRD.md

## What to build

Finish the market data architecture deepening by removing or privatizing obsolete low-level public helpers, deleting obsolete fetcher/subprocess entry points, ensuring callers import through the market data Module root, and keeping implementation documentation minimally correct after Python removal.

## Acceptance criteria

- [x] Obsolete low-level public helpers are deleted or made private after all callers move to the new market data Interface.
- [x] The old public `PriceFetcher` shape is removed after callers use `MarketData` and `MarketDataSources`.
- [x] No Python scripts directory, `uv` subprocess references, or Python fallback paths remain after Rust feature parity exists.
- [x] `utils::resolve_scripts_dir()` and script-resolution documentation are deleted if no non-Python code still uses them.
- [x] `AGENTS.md` no longer describes fund/ETF price fetching as Python scripts via `uv run` after the Rust source **Adapter** is active.
- [x] Callers import market-data behaviour through the market data Module root rather than internal submodules.
- [x] Public market-data result types live in the shared model layer; private helper structs stay inside implementation submodules.
- [x] `docs/ARCHITECTURE.md` no longer describes fund/ETF price fetching as Python scripts via `uv run` or `RSTOCK_SCRIPTS_DIR` after the Rust source **Adapter** is active.
- [x] Tests exercise public market data Interfaces and do not depend on private helper paths.
- [x] `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` pass.

## Blocked by

None - can start immediately
