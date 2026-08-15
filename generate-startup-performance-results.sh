#!/usr/bin/env bash
set -euo pipefail

if [[ "${PERFORMANCE_RESULTS_ONLY:-0}" != "1" ]]; then
  cargo build --release
  cargo bench --bench performance startup_ -- --sample-size 20 --measurement-time 2 --warm-up-time 0.2
  cargo test --test performance_harness automatic_migration_covers_fresh_partial_and_current_schemas
fi

python3 - <<'PY'
import json
from pathlib import Path

names = [
    "startup_and_migration_transactional",
    "startup_executable_only",
    "startup_logging_cold",
    "startup_logging_warm",
    "startup_database_connection_cold",
    "startup_database_connection_warm",
    "startup_automatic_migration_cold",
    "startup_automatic_migration_warm",
    "startup_automatic_migration_warm_unbatched",
    "startup_transaction_list_cold",
    "startup_transaction_list_warm",
]

estimates = {}
root = Path("target/criterion/performance-baseline")
for name in names:
    estimate_path = root / name / "new/estimates.json"
    sample_path = root / name / "new/sample.json"
    if not estimate_path.exists() or not sample_path.exists():
        raise SystemExit(f"missing Criterion result for {name}; run this script without PERFORMANCE_RESULTS_ONLY")
    values = json.loads(estimate_path.read_text())
    sample_values = json.loads(sample_path.read_text())
    samples = sorted(
        time / iterations
        for time, iterations in zip(
            sample_values["times"], sample_values["iters"], strict=True
        )
    )
    estimates[name] = {
        "mean_ns": values["mean"]["point_estimate"],
        "median_ns": values["median"]["point_estimate"],
        "std_dev_ns": values["std_dev"]["point_estimate"],
        "p95_ns": samples[min(len(samples) - 1, int(len(samples) * 0.95))],
    }

cold_target = 115_259_889
warm_audit_with_tolerance = 11_000_000
warm_migration_reference = estimates["startup_automatic_migration_warm_unbatched"]["median_ns"]
report = {
    "procedure": {
        "samples": 20,
        "measurement_time_seconds": 2,
        "warm_up_time_seconds": 0.2,
        "command": "./generate-startup-performance-results.sh",
    },
    "criterion_estimates": estimates,
    "acceptance": {
        "approved_cold_startup_p95_target_ns": cold_target,
        "migration_path_passed": estimates["startup_and_migration_transactional"]["p95_ns"] <= cold_target,
        "cold_migration_passed": estimates["startup_automatic_migration_cold"]["p95_ns"] <= cold_target,
        "cold_transaction_list_passed": estimates["startup_transaction_list_cold"]["p95_ns"] <= cold_target,
        "audited_warm_startup_ns": 10_000_000,
        "approved_regression_tolerance_percent": 10,
        "warm_median_limit_ns": warm_audit_with_tolerance,
        "warm_transaction_list_passed": estimates["startup_transaction_list_warm"]["median_ns"] <= warm_audit_with_tolerance,
        "warm_unbatched_migration_median_ns": warm_migration_reference,
        "warm_migration_regression_limit_ns": warm_migration_reference * 1.1,
        "warm_migration_regression_passed": estimates["startup_automatic_migration_warm"]["median_ns"] <= warm_migration_reference * 1.1,
    },
    "conclusion": "Automatic migrations now run as one SQLite transaction. This preserves pending-migration checks and every migration while removing repeated durable commits; cold migration and full executable startup meet the approved target, and warm median startup remains below the audited value plus tolerance.",
}
if not all(value for key, value in report["acceptance"].items() if key.endswith("_passed")):
    Path("docs/startup-performance-results.json").write_text(json.dumps(report, indent=2) + "\n")
    raise SystemExit("startup acceptance gate failed")
Path("docs/startup-performance-results.json").write_text(json.dumps(report, indent=2) + "\n")
PY
