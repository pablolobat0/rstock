# PR77 / PRD69 Ledger Closure And Final Audit

Status: implementation audit complete on `agent/grill-ledger-fold` at
`40ac6c8` (2026-09-09). This document records the documentation/spec closure
for PR77 and its children #55-#61; it does not decide whether the parent PR
should be merged or released.

## Audit Basis

The audit covered the complete `origin/main..HEAD` history (59 commits), the
PRD and child issues, `AGENTS.md`, `CONTEXT.md`, `docs/ARCHITECTURE.md`,
`docs/CONVENTIONS.md`, and all ADRs. The authoritative sources are:

- PRD #69, **Create One Authoritative Transaction Ledger Projection**.
- Child issues #55 (replay), #56 (recording), #57 (edits/deletes), #58 (CSV
  import), #59 (inventory/analytics), #60 (NAV), and #61 (schema boundary).
- ADR-0003, **One Authoritative Transaction Ledger Projection**.
- Root `CONTEXT.md` for domain vocabulary and snapshot relationships.

Prerequisite PRs are verified merged into this branch: #78 at `bd152c3`
(precision), #79 at `6a74b05` (migration atomicity), #80 at `f2b00d7`
(NAV correctness), and #81 at `40ac6c8` (replay performance). No code or
database migration was changed by this closure.

## Boundary And Approved Decisions

### Ledger Module Interface

`src/services/ledger.rs` is the pure, synchronous boundary for one asset's
Transaction ledger. `CanonicalLedger::from_transactions()` sorts persisted
entries by `(date, id)`, converts them to `LedgerEntryKind`, and validates
asset identity, positive entry identity, and canonical dates. Its opaque entry
vector prevents consumers from depending on incidental repository order.
`replay_transactions()` then validates every chronological prefix and returns
no partial replay after the first violation.

The replay output is semantic rather than persistence-shaped:

- `LedgerTransition` carries the entry, quantity before/after, remaining cost
  before/after, and one typed `LedgerEffect`.
- Buy effects carry native-currency contribution (unit price times units plus
  buy fees).
- Sell effects carry native-currency withdrawal and proportional cost removal;
  sell price and fees do not change remaining cost.
- Dividend effects carry Net dividend income from Gross dividend distribution
  less deductions.
- Split effects carry the new-units-per-old-unit ratio and preserve total cost.

The module has no database, market-data, NAV, or Asset classification
dependency. `LedgerEntry::from_transaction()` is the sole persistence-to-typed
conversion boundary; consumers do not reinterpret overloaded columns.

### Typed-Data Placement Reconciliation

The PRD's general model-layer wording for typed transaction/projection data is
not silently rewritten. The coordinator-approved implementation exception is
that `LedgerEntry`, `LedgerEntryKind`, `LedgerEffect`, `LedgerTransition`, and
their replay results remain colocated with the pure rules in
`src/services/ledger.rs`. The DB-backed `models::Transaction` remains the
persistence-facing model and the display/input models remain in `models/`.
This placement keeps the deep module's interface and invariants together and
is consistent with ADR-0003's service-layer decision.

### Native Currency And FX Enrichment

All transaction monetary components are native to the Tracked asset. After
MarketData prepares historical FX, `enrich_replay()` performs one pure
in-memory lookup using the latest rate on or before each transaction date.
EUR uses the implicit rate `1.0`; a later/current rate is never substituted.
Missing FX makes only dependent Base currency cost, withdrawal, or dividend
facts unavailable while preserving quantity and independent valuation facts.
Market-data acquisition, caching, source coordination, and limitation policy
remain outside replay.

The persisted names ending in `_cents` retain their existing integer scale:
`MONETARY_MULTIPLIER = 10_000.0` means one persisted unit is `0.0001` of the
native currency, not a schema change to ordinary two-decimal cents. Values are
rounded to that scale on input, converted back by division on reads, and
exported with four decimal places. This clarifies the historical field naming
without changing the scale, migration meaning, or user-facing monetary unit.

### Zero-NAV Policy

The approved policy is narrow and explicit. A finite zero NAV with positive
outstanding shares is a valid complete snapshot after canonical epsilon closure
(for example, NAV `0.0` with `0.5` outstanding shares); finite zero asset/total
values are retained. A contribution while outstanding shares are positive and
NAV is non-positive or non-finite is an actionable error. The engine does not
rebase NAV, reset history, erase the valid zero snapshot, or apply quantity
epsilon to NAV state. A fresh contribution is initialized at `INITIAL_NAV`
only when outstanding shares are exactly zero.

Evidence: `src/services/nav.rs:566-623,636-653`,
`tests/nav_tests.rs:1089-1163,1165-1224`. These tests cover full and
incremental replay, zero snapshots, liquidation, and rejected reopening or
contribution after zero NAV.

## Operational Flow

### Mutations

Buy, sell, dividend, split, edit, delete, and CSV import validate input shape,
open a caller-owned database transaction, tentatively write, reload the
complete affected ledger, canonicalize/replay it from zero, invalidate all
dependent Complete NAV snapshots, and commit only after success. Split
mutations additionally clear the asset's adjusted price cache and invalidate
from the asset's earliest transaction date. Persistence, replay, and
invalidation failures roll back together. Generated IDs establish exact
same-date order; edits keep their ID and therefore use `(new date, existing
ID)` order.

CSV rows are sorted by date/source row before insertion, preserving same-date
source order through generated IDs. All affected assets, including existing
later entries, are replayed before any import result is returned. One bad asset
rolls back every imported asset, row, and invalidation.

### NAV And Incremental Readiness

`nav::ensure_portfolio_history()` owns readiness through the latest completed
Effective valuation date. It trusts the latest Complete NAV snapshot as the
checkpoint, prepares one immutable plan with canonical replay transitions and
Positive-holding intervals, and executes the prepared suffix without database
or MarketData reads. NAV alone owns Initial NAV, outstanding shares, share
issuance/redemption, dividend cash, daily valuation, and snapshot persistence.
Asset classification is applied outside replay so Monetary holdings use the
same ledger arithmetic but are excluded from NAV.

Snapshots are not sparse or lazy: a persisted portfolio row and all required
per-asset rows for that date are the atomic rebuild unit. Rebuild writes are
validated for finite/non-negative financial state and committed in batches;
interruption leaves only complete committed batches.

### CLI And Persistence Boundary

The retained command surface is `get`, `portfolio get`, `portfolio asset`,
`transaction buy|sell|dividend|split|list|edit|delete|import|export`,
`analyze composition|fund|correlation`, and `compare funds`. CSV import/export
uses the classified 15-column schema. Removed top-level `buy`, `portfolio
list`, broad `data`, and `monitor` paths are not active features.

Migration `m20260905_000001_contract_transaction_schema` rebuilds the SQLite
table in both directions. The semantic columns are `units` and
`unit_price_cents` for trades, `dividend_amount_cents` and
`dividend_deductions_cents` for dividends, `split_ratio` for splits, and
`fees_cents` for trades. Shape checks reject irrelevant fields, invalid signs,
unknown types, and deductions above gross. IDs, asset IDs, dates, timestamps,
native integer values, global/per-asset chronology indexes, and legacy meaning
are preserved. Public `migration::Migrator::up()` and `down()` wrap a bare
connection in one transaction, including migration bookkeeping. The standard
CLI (`cargo run -- up` or `cargo run -- down` from `migration/`) uses these
entry points. An existing caller-owned transaction, including the one opened
by `db::migrate()`, is reused without an independent commit. Failed up/down
rebuilds roll back schema, data, indexes, sequence, and bookkeeping and can be
retried. This boundary does not promise whole-command atomicity for the
standard CLI's separate `fresh`, `refresh`, or `reset` operations.

## Requirement Matrix

| Scope | Acceptance covered | Evidence | Status |
| --- | --- | --- | --- |
| #55 replay | Canonical order, typed effects, prefix validation, epsilon normalization, actionable first error | `src/services/ledger.rs:141-397,525-805`; `tests/ledger_replay_tests.rs`; inline ledger tests | Verified |
| #56 recording | Type-shape validation, generated-ID order, full replay, atomic invalidation/rollback | `src/services/transactions.rs:42-333`; `tests/transaction_edit_delete_tests.rs:174-290,428-560`; JSON CLI tests | Verified |
| #57 edits/deletes | ID/type/asset protection, chronology edits, suffix validation, split cache/history scopes | `src/services/transactions.rs:225-333`; `tests/transaction_edit_delete_tests.rs:292-426` | Verified |
| #58 CSV import | Classified schema, source ordering, all-asset atomic replay, source-row diagnostics, round trip | `src/services/import.rs:46-321,402-621`; `tests/import_tests.rs` (29 tests) | Verified |
| #59 inventory/analytics | Shared performance/Monetary projection, native FX enrichment, independent nullable facts, open-asset selection | `src/services/portfolio.rs:154-537`; `src/services/analytics.rs:24-103`; `tests/current_positions_tests.rs`, `tests/correlation_tests.rs` | Verified |
| #60 NAV | Semantic daily effects, unitization ownership, Monetary exclusion, seeded/incremental behavior, complete batch writes | `src/services/nav.rs:52-135,244-733`; `tests/nav_tests.rs` (52 tests), `tests/dividend_tests.rs` | Verified |
| #61 schema | Semantic columns/checks, reversible meaning-preserving migration, chronology indexes, no obsolete folds | `migration/src/m20260905_000001_contract_transaction_schema.rs:13-190`; `tests/transaction_schema_migration_tests.rs` (6 tests) | Verified |
| R77 prerequisites | Precision, atomic migration, zero-NAV guard, replay work reduction | merged commits `bd152c3`, `6a74b05`, `f2b00d7`, `40ac6c8` | Verified merged |

## Independent Final Audit

### Original Blocker Root Causes

| Root cause | Corrective implementation | Evidence/status |
| --- | --- | --- |
| Epsilon split closure left zero NAV with outstanding shares, then a new buy issued infinite shares | Reject impossible issuance explicitly and validate finite state before persistence; preserve valid zero-value history | PR #80; `src/services/nav.rs`; full/incremental rejected-contribution and zero-snapshot tests in `tests/nav_tests.rs` |
| Suffix effects lacked metadata for fully sold Monetary assets and positive subepsilon buys | Include every suffix effect asset in metadata scope independently of open valuation holdings | PR #80; `nav_market_data_availability()`; Monetary full/incremental equality and subepsilon contribution tests |
| Standard SQLite migration CLI did not wrap destructive rebuilds and bookkeeping atomically | Public Migrator up/down adds a bare-connection transaction and reuses caller transactions | PR #79; `migration/src/lib.rs`; failed-up/down rollback-and-retry tests in `tests/transaction_schema_migration_tests.rs` |

### Eight `b007d2f` Correction Areas

| Area | Verified evidence | Finding |
| --- | --- | --- |
| Sequence preservation | Migration preserves `MAX(seq)` and restores it above `MAX(id)`; empty/deleted-highest cases covered by `tests/transaction_schema_migration_tests.rs:177-232` | No defect found |
| Receipt semantics | Import receipts derive from persisted replay transitions, including quantized monetary effects and generated IDs: `src/services/import.rs:228-299,518-579`; `tests/import_tests.rs:95-137` | No defect found |
| Split placeholders | Service and CSV paths require split Price/Fees placeholders to be zero: `src/services/transactions.rs:500-504`, `src/services/import.rs:459-480`; `tests/import_tests.rs:457-486` | No defect found |
| Cost closure | Full liquidation resets remaining cost, including after epsilon split closure, and reopening starts fresh: `src/services/ledger.rs:464-506`; `tests/ledger_replay_tests.rs:185-227`, `tests/current_positions_tests.rs:493-514` | No defect found |
| Position visibility | Current positions are projected independently of NAV readiness and retain known quantity/current value: `src/services/portfolio.rs:30-55,154-268`; `tests/current_date_tests.rs:121-159`, `tests/portfolio_summary_tests.rs` | No defect found |
| Canonical NAV quantity | NAV inserts each transition's canonical `quantity_after` and does not reapply split/sell arithmetic: `src/services/nav.rs:549-626`; `tests/nav_tests.rs:1089-1163` | No defect found |
| Audit scans | Audit uses ordered snapshot dates, canonical transitions/cursors, and indexed query-plan checks; `src/services/nav.rs:141-230`, `tests/performance_harness.rs:81-137` | Financial-field scope is intentionally limited; see residual |
| Constraint/assertion specificity | Semantic checks are type-specific and tests assert SQLite CHECK failures rather than generic errors: `migration/src/m20260905_000001_contract_transaction_schema.rs:103-134`, `tests/transaction_schema_migration_tests.rs:8-160` | No defect found |

### Findings And Residuals

**No release blocker found in the reachable normal application paths.** The
full offline suite passed; precision, atomicity, chronology, rollback,
independent FX facts, Monetary exclusion, complete-or-unavailable aggregates,
and full/seeded NAV behavior were all exercised.

**Optional integrity-hardening limitation:**
`find_first_incomplete_snapshot()` checks expected per-asset identity and
canonical quantity, while rebuild writers validate finite/non-negative state.
It does not recalculate arbitrary persisted `portfolio_history` financial
fields or per-asset price/value/FX fields during warm resume. No reachable
normal-path defect was demonstrated by this finding; snapshot writes are
generated and transactional. Arbitrary manual database corruption is outside
the approved audit scope. The current tests intentionally cover missing/wrong asset
rows and quantities (`tests/nav_tests.rs:361-475`), not arbitrary financial
field tampering. Do not claim the warm audit repairs such tampering unless a
future decision expands its scope.

The implementation also intentionally assumes pre-existing persisted ledgers
are valid; migration does not grandfather, clean up, or automatically repair
legacy invalid rows, as recorded in ADR-0003.

## Measured Performance

`docs/replay-performance-evidence.md` is the R77-D timing artifact. It uses
offline deterministic fixtures, the same host/toolchain/SQLite, Criterion
sample size 10, 50 ms measurement and warm-up, and fixture construction outside
timed closures. The corrected-head intervals are:

| Path | Scale | Corrected-head interval |
| --- | --- | ---: |
| Warm readiness | 50 assets / 10 years / 5,000 transactions | 407.07–417.97–429.30 ms |
| Incremental rebuild | 5 assets / 1 year / 100 transactions | 13.962–16.641–20.817 ms |
| Full rebuild | 5 assets / 1 year / 100 transactions | 70.405–75.513–81.503 ms |
| Representative full rebuild | 50 assets / 10 years / 5,000 transactions | 5.3693–5.6011–5.8038 s |
| Stress full rebuild | 100 assets / 20 years / 20,000 transactions | 20.906–21.606–22.265 s |

The artifact records low/point/high wall-clock intervals, not fabricated
targets. Warm readiness remains dominated by the correctness-preserving
complete-history audit; full rebuild remains dominated by calendar-day and
per-asset snapshot work. No seeded replay interface, persisted read model,
sparse snapshot, skipped audit, or market-fetch relocation is justified by the
measurements. Cross-NAV/current-position replay sharing remains deferred because
the required FX coverage and no-replay-payload seam would add complexity
without measured benefit.

## Verification Record

The worker reported passing offline tests, all-target Clippy/check, formatting,
and diff checks. The coordinator independently verified the combined integration
commit `40ac6c8` with networking disabled:

```text
unshare -Urn env CARGO_BUILD_JOBS=2 cargo test --offline --locked --workspace --quiet
unshare -Urn env CARGO_BUILD_JOBS=2 cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo fmt --all --check
git diff --check
```

All passed. Subsequent closure changes are documentation-only; they do not
change the verified source tree. The worker's interrupted publication was
completed by the coordinator after a reboot.
