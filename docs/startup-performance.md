# Performance 12 startup evidence

Issue #31 audits and optimizes startup. The reproducible
procedure is `./generate-startup-performance-results.sh`; its numeric output is
`docs/startup-performance-results.json`. It runs only the startup Criterion
paths against temporary file-backed SQLite databases and dummy source settings.
The executable source URLs use the unsupported `file` scheme, and the measured
low-work commands must complete without invoking a source, so the procedure
does not access the user database or make network requests.

## Measurement seams

- `startup_executable_only` runs the release executable through Clap help, which
  exits before logging or database setup.
- `startup_logging_cold` and `startup_logging_warm` run a dedicated release
  helper that calls the production `logging::init` function, including global
  subscriber installation, against temporary homes.
- `startup_database_connection_cold` and
  `startup_database_connection_warm` perform the production connection call on
  temporary SQLite files without running migrations.
- `startup_automatic_migration_cold` times all migrations on a fresh connected
  database; `startup_automatic_migration_warm` times the transactional
  pending-migration check on a current connected database. The adjacent
  `startup_automatic_migration_warm_unbatched` control measures the old call in
  the same run for a direct warm-regression comparison.
- `startup_transaction_list_cold` and `startup_transaction_list_warm` execute
  the complete empty-list command in fresh and already-migrated temporary homes.
- The original `startup_and_migration` path retains the approved 115,259,889 ns
  p95 target established by issue #20. The startup-specific paths in this
  report are later evidence paths and are informational rather than acceptance
  gates.

## Result

The pre-change diagnostic run measured cold migration at 135,779,249 ns p95 and
cold empty-list startup at 160,789,895 ns p95, both above the approved
115,259,889 ns startup target. Logging and SQLite connection setup remained
immaterial, while cold migration dominated the executable path.

Automatic migrations now run in one SQLite transaction. The migration sequence
and pending-migration check are unchanged, but fresh and partially migrated
databases no longer durably commit each migration separately. The startup
specific measurements are retained as informational evidence; the immutable
issue #20 target is enforced only by `generate-performance-baseline.sh` for the
original `startup_and_migration` path.
Process-level logging initialization is approximately 0.77 ms, while SQLite
connection and current-schema migration checks remain individually small.

The performance harness additionally verifies that automatic migration handles
every historical migration prefix through the current schema while preserving
representative asset and Transaction ledger rows. Existing executable tests
cover the unchanged empty transaction-list and empty portfolio-dashboard output
contracts. The optimization does not cache or bypass migration state, so no
pending migration can be silently skipped.
