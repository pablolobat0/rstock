# Performance 13 contract verification

Issue #32 closes the persistence expand-contract migration on top of the approved
PRD #19 performance baseline. Repository `upsert` and `upsert_many` operations now
use native SQLite conflict handling directly. The former manual check-then-write
implementations and their compatibility names are removed. Historical market-data
writes remain `insert_many_immutable`, because completed-date observations are
immutable during routine commands and failure markers have distinct replacement
semantics.

## No-Change Evidence

- Transaction ledger writes remain single-row inserts or bulk inserts, not upserts:
  ledger IDs and `(date, id)` ordering are externally meaningful, so changing that
  contract would not be a valid persistence optimization.
- Asset creation keeps its duplicate-ticker validation behavior: callers must receive
  the existing user-facing error rather than silently reuse an asset. It is a create
  operation, not an upsert, so native-conflict upsert semantics do not apply.
- Fund holdings snapshots remain append-only history keyed by reported snapshot date;
  replacing or deduplicating that fact would change the analysis history contract.

## Verification

The final record is generated from the approved offline benchmark and startup
procedures, not hand-authored timing claims:

- `./generate-performance-baseline.sh` reruns all approved small, representative,
  and stress timing paths, source-call/concurrency counters, query-plan checks, and
  the complete offline test suite.
- `./generate-startup-performance-results.sh` reruns executable, logging, database,
  migration, and transaction-list startup paths against temporary SQLite databases.
- `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test` are required gates.

Both procedures use dummy identities, injected or unreachable offline sources, and
temporary or in-memory SQLite. They do not access the user database or make network
calls. The generated numeric evidence is stored in
`docs/performance-baseline-results.json` and `docs/startup-performance-results.json`.

The repository persistence tests cover caller-owned transaction rollback, complete
NAV snapshot atomicity, native conflict updates, and equivalence between canonical
single-row and bulk writes. Existing market-data, NAV, ledger-ordering, effective
valuation-date, Base currency, and complete-aggregate tests remain unchanged and are
included in the full offline suite.

The final baseline rerun recorded zero warm Historical market-data source calls,
eight calls with observed concurrency peaks of 1, 2, 4, and 8 for the four delayed
candidates, and successful query-plan and rolling allocation checks. The startup
rerun recorded a 3,083,339 ns warm transaction-list median and a 113,415 ns
transactional warm migration median versus 178,282 ns for the unbatched control.
The benchmark candidate selector reported 8 as fastest in this noisy run, but the
production source limit remains the approved 4 because this issue contracts
persistence APIs and does not change source-concurrency behavior.

The approved baseline contains no numeric application query-count target. The
verification therefore records the five representative `EXPLAIN QUERY PLAN`
statements and their indexed access results instead of inventing a query-count
contract; this is an evidence-based no-change decision, not an omitted check.
