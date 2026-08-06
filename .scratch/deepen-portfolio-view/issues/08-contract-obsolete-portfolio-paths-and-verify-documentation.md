# 08 — Contract obsolete portfolio paths and verify documentation

**What to build:** Complete the deepening by removing obsolete portfolio rebuild and broad current-position paths after all callers have migrated, then align documentation and verification with the final Portfolio view interface and domain behavior.

**Blocked by:** 02 — Give the NAV module readiness ownership; 05 — Make unavailable positions and aggregates explicit; 06 — Route composition through current positions; 07 — Honor capability-based ETF Individual prices

**Status:** ready-for-agent

- [ ] No caller uses a portfolio-owned NAV rebuild prerequisite.
- [ ] No caller requests current positions by receiving a full Portfolio view result with unrelated NAV and risk fields empty.
- [ ] Obsolete public rebuild orchestration and shallow current-position entry points are removed.
- [ ] The portfolio module interface exposes only the accepted focused current-positions and full Portfolio view outcomes needed by callers.
- [ ] No duplicate performance-versus-Monetary ledger calculation remains.
- [ ] No current-holding path substitutes current FX for missing historical transaction FX.
- [ ] No current-holding path adds dividends to Open-position gain/loss.
- [ ] No current performance holding is omitted solely because its Individual price is unavailable.
- [ ] Architecture documentation describes current positions, full Portfolio view enrichment, NAV readiness ownership, complete-or-unavailable aggregates, and limitation scopes.
- [ ] Conventions describe fixed-clock portfolio tests and the portfolio interface as the test surface.
- [ ] Documentation reflects capability-based ETF Individual price behavior without contradicting ADR-0001.
- [ ] All superseded tests and documentation statements are updated or removed.
- [ ] `cargo fmt && cargo clippy -- -D warnings` passes.
- [ ] `cargo test` passes without network calls.
